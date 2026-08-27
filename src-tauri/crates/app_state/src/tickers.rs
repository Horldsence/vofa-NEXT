//! 后台推送 ticker — 由 lib.rs::run 在 .setup 阶段 spawn
//!
//! 自适应速率由 [`pipeline_stream::AdaptiveRate`] 提供; 频谱/Ifft 同步
//! 由 [`pipeline_dispatcher::sync_spectrum_analyzers`] /
//! [`pipeline_dispatcher::sync_ifft_buffers`] 提供。

use crate::{CustomInputBatch, GraphEvalState, SpectrumBatch};
use dsp_fft::SpectrumResult;
use pipeline_data_plane::StreamGroupState;
use pipeline_dispatcher::{sync_ifft_buffers, sync_spectrum_analyzers};
use pipeline_stream::AdaptiveRate;
use std::collections::HashMap;
use std::time::Duration;

/// 合并自定义文本输出与后端图求值字符串输出 — 同 (widget, port) 键以后端求值为准
///
/// custom_text_outputs: 前端 submit_custom_text_output 写入 (Trigger/Custom JS 节点)
/// graph_string_outputs: 后端图求值写入 (Str 节点, 见 graph_eval 发布点)
/// 逐端口深合并: 同 widget 不同 port 并存, 同 port 时 graph 覆盖 custom
fn merge_string_outputs(
    custom: &HashMap<String, HashMap<String, String>>,
    graph: &HashMap<String, HashMap<String, String>>,
) -> HashMap<String, HashMap<String, String>> {
    let mut merged = custom.clone();
    for (node_id, ports) in graph {
        let entry = merged.entry(node_id.clone()).or_default();
        for (port, value) in ports {
            entry.insert(port.clone(), value.clone());
        }
    }
    merged
}

/// 字符串输出推送循环 — 自适应速率推送 custom_text_outputs ⊕ graph_string_outputs
/// 合并视图 (同键以后者为准) 到所有订阅者
///
/// 订阅者通过 invoke('subscribe_string_outputs', on_event: Channel) 加入
/// Channel 关闭时自动移除
///
/// 自适应: 内容与上次发送相同 → 不发送并降频退避 (最高 250ms);
/// 有变化 → 立即发送并提速 (最快 33ms, ~30 FPS, 字符串变化频率低于数字)
pub async fn text_output_ticker(state: GraphEvalState) {
    log::debug!("字符串输出 ticker 已启动 (自适应 33ms~250ms)");
    let mut rate = AdaptiveRate::new(Duration::from_millis(33), Duration::from_millis(250));

    loop {
        tokio::time::sleep(rate.current()).await;
        // 把当前 custom_text_outputs ⊕ graph_string_outputs 合并视图同步到快照 (递增 tick)
        let snap = {
            let custom = state.custom_text_outputs.lock().clone();
            let graph = state.graph_string_outputs.lock().clone();
            let current = merge_string_outputs(&custom, &graph);
            let mut s = state.text_output_snapshot.lock();
            let changed = s.values != current;
            if !changed && s.tick > 0 {
                drop(s);
                rate.on_idle();
                continue;
            }
            s.tick = s.tick.wrapping_add(1);
            s.values = current;
            s.clone()
        };
        let mut subs = state.text_output_subscribers.lock();
        if subs.is_empty() {
            rate.on_idle();
            continue;
        }
        subs.retain(|ch| ch.send(snap.clone()).is_ok());
        rate.on_send();
    }
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

/// 抑制未用警告 — `StreamGroupState` 由 `AppState::new` 持有, ticker 自身不直接使用,
/// 但保留 import 以确认该类型的可见性。
#[allow(dead_code)]
fn _force_import(_: StreamGroupState) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn str_map(entries: &[(&str, &str, &str)]) -> HashMap<String, HashMap<String, String>> {
        let mut m: HashMap<String, HashMap<String, String>> = HashMap::new();
        for &(node, port, value) in entries {
            m.entry(node.to_string())
                .or_default()
                .insert(port.to_string(), value.to_string());
        }
        m
    }

    #[test]
    fn merge_prefers_graph_on_same_key() {
        let custom = str_map(&[("w1", "text", "custom"), ("w1", "extra", "keep")]);
        let graph = str_map(&[("w1", "text", "graph")]);
        let merged = merge_string_outputs(&custom, &graph);
        // 同 (widget, port) 键: 后端求值覆盖前端提交
        assert_eq!(merged["w1"]["text"], "graph");
        // 同 widget 不同 port: 并存 (深合并, 非整节点覆盖)
        assert_eq!(merged["w1"]["extra"], "keep");
    }

    #[test]
    fn merge_keeps_disjoint_entries() {
        let custom = str_map(&[("trig1", "text", "hit")]);
        let graph = str_map(&[("str1", "result", "ABC")]);
        let merged = merge_string_outputs(&custom, &graph);
        assert_eq!(merged["trig1"]["text"], "hit");
        assert_eq!(merged["str1"]["result"], "ABC");
    }

    #[test]
    fn merge_handles_empty_sides() {
        let custom = str_map(&[("trig1", "text", "hit")]);
        assert_eq!(merge_string_outputs(&custom, &HashMap::new()), custom);
        let graph = str_map(&[("str1", "result", "ABC")]);
        assert_eq!(merge_string_outputs(&HashMap::new(), &graph), graph);
    }
}
