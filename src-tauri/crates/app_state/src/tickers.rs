//! 后台推送 ticker — 由 lib.rs::run 在 .setup 阶段 spawn
//!
//! 自适应速率由 [`pipeline_stream::AdaptiveRate`] 提供; 频谱/Ifft 同步
//! 由 [`pipeline_dispatcher::sync_spectrum_analyzers`] /
//! [`pipeline_dispatcher::sync_ifft_buffers`] 提供。

use crate::GraphEvalState;
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
/// 订阅者通过统一的 `subscribe_display` 命令读取快照。
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
        drop(snap);
        rate.on_send();
    }
}

/// 频谱分析推送循环 — 自适应速率触发 FFT + 推送结果到所有订阅者
///
/// 订阅者通过统一的 `subscribe_display` 命令读取快照。
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

        rate.on_send();
    }
}

/// 抑制未用警告 — `StreamGroupState` 由 `AppState::new` 持有, ticker 自身不直接使用,
/// 但保留 import 以确认该类型的可见性。
#[allow(dead_code)]
fn _force_import(_: StreamGroupState) {}

/// TextOut 发送循环 — 图内字符串 (TextOut 节点) 限速写回目标 Transport 的 tx
///
/// 数据来源: `graph_string_outputs[textout_id]["text"]` (通用 materialize_str 发布点写入);
/// 规格: `CompiledEval::textouts()` (编译期收集 target_transport / newline / min_interval)。
///
/// 发送条件: 文本相对上次已发送值发生变化, 且距上次发送 ≥ min_interval_ms;
/// 未到窗口时保持 dirty, 下轮窗口满足后补发。目标未打开等发送失败按 min_interval
/// 节奏重试并限频记日志。无 TextOut / 无变化时空转 (10ms tick)。
pub async fn textout_sender_ticker(
    state: GraphEvalState,
    transport: std::sync::Arc<tokio::sync::Mutex<transport_core::TransportManager>>,
) {
    use std::time::Instant;

    /// 单节点发送状态机
    struct NodeTx {
        sent_value: String,
        last_send: Option<Instant>,
        dirty: bool,
        err_logged: bool,
    }
    let mut txs: HashMap<String, NodeTx> = HashMap::new();
    log::debug!("TextOut 发送 ticker 已启动 (10ms tick)");

    loop {
        tokio::time::sleep(Duration::from_millis(10)).await;

        // 锁内仅收集待发列表 (锁外执行 IO)
        // (textout node_id 状态键, 目标 transport id, 已含换行的 payload, min_interval_ms)
        let mut candidates: Vec<(String, String, String, u32)> = Vec::new();
        {
            let graphs = state.graphs.lock();
            if graphs.values().all(|g| g.compiled().textouts().is_empty()) {
                continue;
            }
            let out = state.graph_string_outputs.lock();
            for g in graphs.values() {
                for spec in g.compiled().textouts() {
                    if let Some(text) = out.get(&*spec.node_id).and_then(|p| p.get("text")) {
                        let mut value =
                            String::with_capacity(text.len() + spec.newline_suffix.len());
                        value.push_str(text);
                        value.push_str(spec.newline_suffix);
                        candidates.push((
                            spec.node_id.to_string(),
                            spec.target_transport.to_string(),
                            value,
                            spec.min_interval_ms,
                        ));
                    }
                }
            }
        }
        if candidates.is_empty() {
            continue;
        }

        let now = Instant::now();
        for (node_id, target, value, interval_ms) in candidates {
            let min_wait = Duration::from_millis(u64::from(interval_ms));
            let entry = txs.entry(node_id.clone()).or_insert(NodeTx {
                sent_value: String::new(),
                last_send: None,
                dirty: false,
                err_logged: false,
            });
            if entry.sent_value != value {
                entry.dirty = true;
            }
            if !entry.dirty {
                continue;
            }
            if entry
                .last_send
                .is_some_and(|t| now.duration_since(t) < min_wait)
            {
                continue; // 未到窗口: 保持 dirty 待补发
            }
            let send_result = transport.lock().await.send(&target, value.as_bytes());
            match send_result {
                Ok(()) => {
                    entry.sent_value = value;
                    entry.last_send = Some(now);
                    entry.err_logged = false;
                }
                Err(_e) => {
                    entry.last_send = Some(now); // 失败也按窗口节奏重试
                    if !entry.err_logged {
                        log::warn!("TextOut {node_id}: 目标不可达, 将按间隔重试");
                    }
                    entry.err_logged = true;
                }
            }
        }
    }
}

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
