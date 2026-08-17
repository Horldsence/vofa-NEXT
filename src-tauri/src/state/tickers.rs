use crate::pipeline::dispatcher::AdaptiveRate;
use crate::state::app_state::*;
use std::collections::HashMap;
use std::time::Duration;
use vofa_next_dsp::SpectrumResult;

/// 同步 spectrum_analyzers 与 graphs — 委托到 pipeline::spectrum_sync
fn sync_spectrum_analyzers(state: &GraphEvalState) {
    crate::pipeline::spectrum_sync::sync_spectrum_analyzers(state);
}

/// 同步 Ifft 节点重建缓冲 — 委托到 pipeline::spectrum_sync
fn sync_ifft_buffers(state: &GraphEvalState) {
    crate::pipeline::spectrum_sync::sync_ifft_buffers(state);
}

/// 图输出推送循环 — 自适应速率推送 output_snapshot 到所有订阅者
///
/// 订阅者通过 invoke('subscribe_graph_outputs', on_event: Channel) 加入
/// Channel 关闭时自动移除
///
/// 自适应: snapshot.tick 未变化 → 不发送并降频退避 (最高 250ms);
/// 有变化 → 立即发送并提速 (最快 16ms, ~60 FPS)
pub async fn graph_output_ticker(state: GraphEvalState) {
    log::debug!("图输出 ticker 已启动 (自适应 16ms~250ms)");
    let mut rate = AdaptiveRate::new(Duration::from_millis(16), Duration::from_millis(250));
    let mut last_sent_tick: Option<u64> = None;

    loop {
        tokio::time::sleep(rate.current()).await;
        // 变化检测: tick 未变 → 无新求值结果, 跳过
        let (tick, snap) = {
            let s = state.output_snapshot.lock();
            (s.tick, s.clone())
        };
        if last_sent_tick == Some(tick) {
            rate.on_idle();
            continue;
        }
        let mut subs = state.output_subscribers.lock();
        // 尝试推送, 失败 (Channel 关闭) 则移除
        subs.retain(|ch| ch.send(snap.clone()).is_ok());
        last_sent_tick = Some(tick);
        rate.on_send();
    }
}

/// Custom 输入推送循环 — 自适应速率推送 Custom 输入到所有订阅者
///
/// 订阅者通过 invoke('subscribe_custom_inputs', on_event: Channel) 加入
///
/// 自适应: 输入值与上次发送相同 → 不发送并降频退避 (最高 250ms)
pub async fn custom_input_ticker(state: GraphEvalState) {
    log::debug!("Custom 输入 ticker 已启动 (自适应 33ms~250ms)");
    let mut rate = AdaptiveRate::new(Duration::from_millis(33), Duration::from_millis(250));
    let mut last_sent: Option<HashMap<String, HashMap<String, f32>>> = None;

    loop {
        tokio::time::sleep(rate.current()).await;
        // 仅当存在 Custom 节点时才收集
        let has_custom = state
            .graphs
            .lock()
            .values()
            .any(|g| !g.custom_node_ids().is_empty());
        if !has_custom {
            rate.on_idle();
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
            rate.on_idle();
            continue;
        }
        // 变化检测: 与上次发送相同 → 跳过
        if last_sent.as_ref() == Some(&inputs) {
            rate.on_idle();
            continue;
        }
        let batch = CustomInputBatch {
            inputs: inputs.clone(),
        };
        let mut subs = state.custom_input_subscribers.lock();
        subs.retain(|ch| ch.send(batch.clone()).is_ok());
        last_sent = Some(inputs);
        rate.on_send();
    }
}

/// 频谱分析推送循环 — 自适应速率触发 FFT + 推送结果到所有订阅者
///
/// 订阅者通过 invoke('subscribe_spectrum', on_event: Channel) 加入
/// Channel 关闭时自动移除
///
/// 流程:
/// 1. 每 tick 开头调用 sync_spectrum_analyzers 与 graphs 同步
/// 2. 对每个 analyzer 调用 compute() (窗口未填满返回 None, 跳过)
/// 3. 将结果存入 spectrum_snapshot
/// 4. 推送 SpectrumBatch 到所有订阅者
///
/// 自适应: 无 analyzer 或无新结果 → 降频退避 (最高 250ms)
pub async fn spectrum_ticker(state: GraphEvalState) {
    log::debug!("频谱分析 ticker 已启动 (自适应 33ms~250ms)");
    let mut rate = AdaptiveRate::new(Duration::from_millis(33), Duration::from_millis(250));

    loop {
        tokio::time::sleep(rate.current()).await;
        // 1. 同步 analyzers 与 graphs
        sync_spectrum_analyzers(&state);
        // 1b. 同步 Ifft 节点重建缓冲与 graphs / 最新频谱
        sync_ifft_buffers(&state);

        // 2. 对每个 analyzer 计算 FFT
        let mut analyzers = state.spectrum_analyzers.lock();
        if analyzers.is_empty() {
            drop(analyzers);
            rate.on_idle();
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
            rate.on_idle();
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
        rate.on_send();
    }
}
