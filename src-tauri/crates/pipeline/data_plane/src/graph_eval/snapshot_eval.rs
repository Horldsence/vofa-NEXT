//! 事件驱动快照评估与连续性状态复位

use std::collections::HashMap;
use std::sync::Arc;

use data_bus::TopicKey;
use engine::SourceFramesMap;
use kind::NodeKind;

use crate::eval_state::GraphEvalState;

use super::hot_path::merge_str_map;
use super::predicates::graph_triggered_by;

/// 事件驱动快照评估 — 以 source_frames 现状评估所有图并发布 output_snapshot
///
/// 步骤:
/// 1. 对每个图调用 evaluate (传入 filter_states + decoder_states + trigger_states,
///    逐点滤波/解码/触发匹配状态跨帧持久化)
/// 2. 合并所有图输出到 output_snapshot
/// 3. 遍历所有图的 Fft, 从 output_snapshot 取输入值, push 到对应 analyzer
///
/// 调用时机: FrameDecoder 字节喂入后 / set_input_value / submit_custom_output
/// (取代旧 evaluate_all_graphs_with 的空帧语义 — ProtocolSource 从缓存读最新值)
pub fn evaluate_snapshot_now(eval_state: &GraphEvalState, source_frames: &SourceFramesMap) {
    let input_values = eval_state.input_values.read();
    let custom_outputs = eval_state.custom_outputs.read();
    let source_texts = eval_state.source_texts.lock();
    let graphs = eval_state.graphs.lock();
    let mut filter_states = eval_state.filter_states.lock();
    let decoder_states = eval_state.decoder_states.lock();
    let mut ifft_states = eval_state.ifft_states.lock();
    let mut trigger_states = eval_state.trigger_states.lock();

    let mut combined: engine::ValuesMap = HashMap::default();
    // 字符串输出: 各图求值结果累积于此, 求值后全量覆盖写进 graph_string_outputs
    let mut combined_str = engine::StringValuesMap::default();
    for (_, graph) in graphs.iter() {
        let out = graph.evaluate(
            source_frames,
            &source_texts,
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &decoder_states,
            &mut ifft_states,
            &mut trigger_states,
            &mut combined_str,
        );
        for (k, v) in out {
            combined.insert(k, v);
        }
    }

    // 更新 output_snapshot (供 60 FPS ticker 推送)
    {
        let mut snap = eval_state.output_snapshot.lock();
        snap.tick = snap.tick.wrapping_add(1);
        // clone_from 复用旧快照的分配; combined 随后仍作为谱输入被读取
        snap.values.clone_from(&combined);
    }

    // Input / Custom / FrameDecoder 等事件驱动求值不经过 process_source_batch，
    // 过去只刷新 latest-value 快照，导致已迁移到 DataBus 的显示节点永远等不到
    // 派生样本。只发布非 ProtocolSource 输出，避免把缓存的协议末值伪造成新采样。
    let event_timestamp = vofa_core::now_us();
    for (node_id, ports) in &combined {
        let is_protocol_source = graphs.values().any(|graph| {
            matches!(
                graph.value_def(node_id).map(|node| &node.kind),
                Some(NodeKind::ProtocolSource { .. })
            )
        });
        if is_protocol_source {
            continue;
        }
        for (port, value) in ports {
            let key = TopicKey::new(node_id, port);
            if eval_state.data_bus.is_active(&key) {
                eval_state.data_bus.publish_samples(
                    key,
                    Arc::from([event_timestamp]),
                    Arc::from([f64::from(*value)]),
                );
            }
        }
    }

    // 更新后端字符串输出 (供 text_output_ticker 合并发布) —
    // 全量覆盖写: combined_str 覆盖所有图, 先物化到本地 map 再整体 swap,
    // 过期节点条目随 swap 清理 (同 snap.values 语义)
    let mut str_map: HashMap<String, HashMap<String, String>> = HashMap::new();
    merge_str_map(combined_str, &mut str_map);
    *eval_state.graph_string_outputs.lock() = str_map;

    // 收集 Fft 输入值, push 到对应 analyzer 的滑动窗口
    // analyzer 的创建/删除由 spectrum_ticker 在每 tick 开头与 graphs 同步
    let mut analyzers = eval_state.spectrum_analyzers.lock();
    if !analyzers.is_empty() {
        for (_, graph) in graphs.iter() {
            let spectrum_inputs = graph.collect_spectrum_inputs(&combined);
            for (sink_id, value) in spectrum_inputs {
                if let Some(analyzer) = analyzers.get_mut(&sink_id) {
                    analyzer.push_with(value, |frame| {
                        for target in graph.spectrum_consumers(&sink_id) {
                            if let Err(error) =
                                ifft_states.entry(target.clone()).or_default().accept(frame)
                            {
                                log::warn!("IFFT {target}: {error}");
                            }
                        }
                    });
                }
            }
        }
    }
}

/// 缺口后的有状态算子复位 (不变量 5) — 求值平面丢弃整批造成时间缺口,
/// 滤波/触发/IFFT 状态失去连续性; 显式复位并告警, 而不是带着断裂状态
/// 继续产出看似连续的近似值 (静默畸变)。
pub fn reset_source_transient_state(eval_state: &GraphEvalState, source_id: &str) {
    let graphs = eval_state.graphs.lock();
    let mut filters: Vec<String> = Vec::new();
    let mut iffts: Vec<String> = Vec::new();
    let mut triggers: Vec<String> = Vec::new();
    for g in graphs.values() {
        if !graph_triggered_by(g, source_id) {
            continue;
        }
        let compiled = g.compiled();
        filters.extend(g.filter_node_ids().iter().cloned());
        iffts.extend(g.ifft_node_ids().iter().cloned());
        for node in g.value_nodes() {
            if matches!(node.kind, NodeKind::Trigger { .. }) {
                triggers.push(node.id.clone());
            }
        }
        let _ = compiled;
    }
    drop(graphs);
    let mut reset = 0_usize;
    if !filters.is_empty() {
        let mut states = eval_state.filter_states.lock();
        for key in &filters {
            if states.remove(key).is_some() {
                reset += 1;
            }
        }
    }
    if !iffts.is_empty() {
        let mut states = eval_state.ifft_states.lock();
        for key in &iffts {
            if states.remove(key).is_some() {
                reset += 1;
            }
        }
    }
    if !triggers.is_empty() {
        let mut states = eval_state.trigger_states.lock();
        for key in &triggers {
            if states.remove(key).is_some() {
                reset += 1;
            }
        }
    }
    if reset > 0 {
        log::warn!(
            "求值缺口: 已复位源 {source_id} 关联的有状态算子 {reset} 项 (滤波/触发/IFFT), \
             后续输出从复位后状态重新连续"
        );
    }
}

/// 工作区级连续性状态复位 — 暂停恢复 / 启动 / 停止时调用。
///
/// 暂停期间字节被显式丢弃 (读任务门控), 字节流与样本时间轴在恢复点断裂;
/// 全部跨帧有状态算子 (滤波延迟线 / IFFT 重叠相加 / 触发边沿 / 帧解码状态机 /
/// FFT 滑窗) 一律清空, 恢复后从新流序列重新连续。各状态均为求值时懒建
/// (或 ticker 每拍与 graphs 同步重建), 清空即复位, 不丢配置。
pub fn reset_all_transient_state(eval_state: &GraphEvalState) {
    let mut reset = 0_usize;
    reset += clear_state_map(&eval_state.filter_states);
    reset += clear_state_map(&eval_state.ifft_states);
    reset += clear_state_map(&eval_state.trigger_states);
    reset += clear_state_map(&eval_state.decoder_states);
    reset += clear_state_map(&eval_state.spectrum_analyzers);
    if reset > 0 {
        log::info!("运行状态切换: 已复位全部连续性状态 {reset} 项 (滤波/IFFT/触发/解码/频谱窗)");
    }
}

/// 清空一个状态 map 并返回清除条目数 (短促持锁, 不与其他锁嵌套)
fn clear_state_map<T>(map: &std::sync::Arc<parking_lot::Mutex<HashMap<String, T>>>) -> usize {
    let mut guard = map.lock();
    let n = guard.len();
    guard.clear();
    n
}
