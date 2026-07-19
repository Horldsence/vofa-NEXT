use crate::state::app_state::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tauri::ipc::Channel;
use tokio::sync::oneshot;
use vofa_next_buffer::{RawDataBatch, RawDataCollector};
use vofa_next_core::{CanBuffer, CanFrameBatch};
use vofa_next_dsp::SpectrumResult;

/// 同步 spectrum_analyzers 与 graphs — 委托到 pipeline::spectrum_sync
fn sync_spectrum_analyzers(state: &GraphEvalState) {
    crate::pipeline::spectrum_sync::sync_spectrum_analyzers(state);
}

/// 图输出推送循环 — 60 FPS 推送 output_snapshot 到所有订阅者
///
/// 订阅者通过 invoke('subscribe_graph_outputs', on_event: Channel) 加入
/// Channel 关闭时自动移除
pub async fn graph_output_ticker(state: GraphEvalState) {
    log::info!("图输出 ticker 已启动 (60 FPS)");
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(16));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        let snap = state.output_snapshot.lock().clone();
        let mut subs = state.output_subscribers.lock();
        // 尝试推送, 失败 (Channel 关闭) 则移除
        subs.retain(|ch| ch.send(snap.clone()).is_ok());
    }
}

/// Custom 输入推送循环 — 30 FPS 推送 Custom 输入到所有订阅者
///
/// 订阅者通过 invoke('subscribe_custom_inputs', on_event: Channel) 加入
pub async fn custom_input_ticker(state: GraphEvalState) {
    log::info!("Custom 输入 ticker 已启动 (30 FPS)");
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(33));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        // 仅当存在 Custom 节点时才收集
        let has_custom = state
            .graphs
            .lock()
            .values()
            .any(|g| !g.custom_node_ids().is_empty());
        if !has_custom {
            continue;
        }
        // 收集 Custom 输入
        let snap = state.output_snapshot.lock();
        let graphs = state.graphs.lock();
        let mut inputs: HashMap<String, HashMap<String, f32>> = HashMap::new();
        for (_, graph) in graphs.iter() {
            let ci = graph.collect_custom_inputs(&snap.values);
            for (k, v) in ci {
                inputs.insert(k, v);
            }
        }
        drop(snap);
        drop(graphs);

        if inputs.is_empty() {
            continue;
        }
        let batch = CustomInputBatch { inputs };
        let mut subs = state.custom_input_subscribers.lock();
        subs.retain(|ch| ch.send(batch.clone()).is_ok());
    }
}

/// 频谱分析推送循环 — 30 FPS 触发 FFT + 推送结果到所有订阅者
///
/// 订阅者通过 invoke('subscribe_spectrum', on_event: Channel) 加入
/// Channel 关闭时自动移除
///
/// 流程:
/// 1. 每 tick 开头调用 sync_spectrum_analyzers 与 graphs 同步
/// 2. 对每个 analyzer 调用 compute() (窗口未填满返回 None, 跳过)
/// 3. 将结果存入 spectrum_snapshot
/// 4. 推送 SpectrumBatch 到所有订阅者
pub async fn spectrum_ticker(state: GraphEvalState) {
    log::info!("频谱分析 ticker 已启动 (30 FPS)");
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(33));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        // 1. 同步 analyzers 与 graphs
        sync_spectrum_analyzers(&state);

        // 2. 对每个 analyzer 计算 FFT
        let mut analyzers = state.spectrum_analyzers.lock();
        if analyzers.is_empty() {
            continue;
        }
        let mut new_results: HashMap<String, SpectrumResult> = HashMap::new();
        for (sink_id, analyzer) in analyzers.iter_mut() {
            if let Some(result) = analyzer.compute() {
                new_results.insert(sink_id.clone(), result);
            }
        }
        drop(analyzers);

        if new_results.is_empty() {
            continue;
        }

        // 3. 更新 spectrum_snapshot
        {
            let mut snap = state.spectrum_snapshot.lock();
            for (k, v) in &new_results {
                snap.insert(k.clone(), v.clone());
            }
        }

        // 4. 推送到所有订阅者 (snapshot 全量推送, 保证新订阅者立即收到数据)
        let batch = SpectrumBatch {
            spectra: state.spectrum_snapshot.lock().clone(),
        };
        let mut subs = state.spectrum_subscribers.lock();
        subs.retain(|ch| ch.send(batch.clone()).is_ok());
    }
}

/// CAN 帧订阅推送循环 — 按 interval_ms 推送最近 max_frames 个 CAN 帧
///
/// 由 subscribe_can_frames 命令 spawn, 通过 oneshot 接收取消信号优雅退出。
/// 推送的是缓冲区快照 (get_recent), 与 data_loop 的实时 emit 互补:
/// - data_loop emit "transport:can-frames": 实时批次 (适合需要逐帧处理的场景)
/// - can_frames_loop via Channel: 周期性快照 (适合 UI 列表展示, 控制刷新频率)
pub async fn can_frames_loop(
    buffer: Arc<Mutex<CanBuffer>>,
    on_event: Channel<CanFrameBatch>,
    interval: Duration,
    max_frames: usize,
    cancel_rx: oneshot::Receiver<()>,
) {
    let mut ticker = tokio::time::interval(interval);
    let mut cancel_rx = cancel_rx;
    loop {
        tokio::select! {
            _ = &mut cancel_rx => break,
            _ = ticker.tick() => {
                let frames = {
                    let buf = buffer.lock();
                    buf.get_recent(max_frames)
                };
                if on_event.send(CanFrameBatch { frames }).is_err() {
                    break;
                }
            }
        }
    }
}

/// 原始数据订阅推送循环 — 按 interval_ms 推送一批原始字节
///
/// 由 subscribe_rawdata 命令 spawn, 通过 oneshot 接收取消信号优雅退出。
/// 从 RawDataCollector 中 drain 最多 max_bytes 的完整块, 通过 Channel 推送到前端。
pub async fn rawdata_loop(
    collector: Arc<Mutex<RawDataCollector>>,
    on_event: Channel<RawDataBatch>,
    interval: Duration,
    max_bytes: usize,
    cancel_rx: oneshot::Receiver<()>,
) {
    let mut ticker = tokio::time::interval(interval);
    let mut cancel_rx = cancel_rx;
    loop {
        tokio::select! {
            _ = &mut cancel_rx => break,
            _ = ticker.tick() => {
                let batch = {
                    let mut col = collector.lock();
                    col.drain_batch(max_bytes)
                };
                if batch.chunks.is_empty() {
                    continue;
                }
                if on_event.send(batch).is_err() {
                    break;
                }
            }
        }
    }
}
