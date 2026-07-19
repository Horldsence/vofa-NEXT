use crate::state::GraphEvalState;
use std::collections::HashMap;
use vofa_next_buffer::DataBuffer;
use vofa_next_core::DataFrame;

/// 评估所有图 (静态函数版本, 供 GraphEvalState 使用)
///
/// 步骤:
/// 1. 对每个图调用 evaluate (传入 filter_states + decoder_states, 逐点滤波/解码跨帧持久化)
/// 2. 合并所有图输出到 output_snapshot
/// 3. 遍历所有图的 SpectrumSink, 从 output_snapshot 取输入值, push 到对应 analyzer
pub fn evaluate_all_graphs_with(eval_state: &GraphEvalState, frame: &DataFrame) {
    let input_values = eval_state.input_values.lock().clone();
    let custom_outputs = eval_state.custom_outputs.lock().clone();
    let graphs = eval_state.graphs.lock();
    let mut filter_states = eval_state.filter_states.lock();
    let mut decoder_states = eval_state.decoder_states.lock();

    let mut combined: HashMap<String, HashMap<String, f32>> = HashMap::new();
    for (_, graph) in graphs.iter() {
        let out = graph.evaluate(frame, &input_values, &custom_outputs, &mut filter_states, &mut decoder_states);
        for (k, v) in out {
            combined.insert(k, v);
        }
    }

    // 更新 output_snapshot (供 60 FPS ticker 推送)
    {
        let mut snap = eval_state.output_snapshot.lock();
        snap.tick = snap.tick.wrapping_add(1);
        snap.values = combined.clone();
    }

    // 收集 SpectrumSink 输入值, push 到对应 analyzer 的滑动窗口
    // analyzer 的创建/删除由 spectrum_ticker 在每 tick 开头与 graphs 同步
    let mut analyzers = eval_state.spectrum_analyzers.lock();
    if !analyzers.is_empty() {
        for (_, graph) in graphs.iter() {
            let spectrum_inputs = graph.collect_spectrum_inputs(&combined);
            for (sink_id, value) in spectrum_inputs {
                if let Some(analyzer) = analyzers.get_mut(&sink_id) {
                    analyzer.push(value);
                }
            }
        }
    }
}

/// 从 output_snapshot 收集派生值, push 到 buffer 的 derived_buffers
///
/// 遍历所有 graph 的 edges, 对每条 edge:
///   若 source 在 output_snapshot 中 (即 source 是有输出的节点: Math/Input/Custom/ChannelSource/FrameDecoder):
///     取 snapshot[source][sourceHandle], push 到 buffer.derived_buffers[(target, source)]
///
/// **时间对齐**: 本函数在每帧 evaluate_all_graphs_with 后调用,
/// 与 push_frame 同步, 保证 derived[i] 与 timestamps[i] 对齐。
pub fn push_derived_from_snapshot(eval_state: &GraphEvalState, buffer: &mut DataBuffer) {
    let snap = eval_state.output_snapshot.lock();
    let graphs = eval_state.graphs.lock();
    for (_, graph) in graphs.iter() {
        for e in graph.edges() {
            // 只对有输出的 source (ChannelSource/Input/Math/Custom) 收集派生值
            if let Some(src_out) = snap.values.get(&e.source) {
                if let Some(&val) = src_out.get(&e.source_handle) {
                    buffer.push_derived(&e.target, &e.source, val);
                }
            }
        }
    }
}
