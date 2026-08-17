use crate::state::GraphEvalState;
use std::collections::HashMap;
use vofa_next_dsp::SpectrumAnalyzer;

/// 同步 spectrum_analyzers 与 graphs 中的 SpectrumSink 节点
///
/// - 遍历所有 graph 的 spectrum_sink_ids, 对每个 sink:
///   - 若 analyzer 不存在 → 按当前 config 创建
///   - 若 analyzer 存在但 config 变了 (window_size/window_type/output/sample_rate) → 重建
/// - 删除 graphs 中已不存在的 sink 对应的 analyzer
/// - 同时清理 spectrum_snapshot 中已不存在的 sink
///
/// 由 spectrum_ticker 在每 tick 开头调用, 保证 analyzer 与图拓扑一致。
pub fn sync_spectrum_analyzers(state: &GraphEvalState) {
    let graphs = state.graphs.lock();
    let mut analyzers = state.spectrum_analyzers.lock();

    // 收集所有 graph 中当前的 SpectrumSink id → config
    let mut current_configs: HashMap<
        String,
        (
            usize,
            vofa_next_dsp::WindowType,
            vofa_next_dsp::SpectrumOutput,
            f32,
        ),
    > = HashMap::new();
    for (_, graph) in graphs.iter() {
        for sink_id in graph.spectrum_sink_ids() {
            if let Some(cfg) = graph.spectrum_sink_config(&sink_id) {
                current_configs.insert(sink_id, cfg);
            }
        }
    }

    // 删除已不存在的 sink 的 analyzer
    analyzers.retain(|id, _| current_configs.contains_key(id));
    {
        let mut snap = state.spectrum_snapshot.lock();
        snap.retain(|id, _| current_configs.contains_key(id));
    }

    // 新建或重建 analyzer
    for (sink_id, (window_size, window_type, output, sample_rate)) in &current_configs {
        let need_rebuild = match analyzers.get(sink_id) {
            None => true,
            Some(a) => {
                // 任一配置变化都需要重建 (window_size/sample_rate 需要 new FFT planner;
                // window_type/output 虽有 setter 但重建更简单且不影响性能)
                a.window_size() != *window_size
                    || a.sample_rate() != *sample_rate
                    || a.window_type() != *window_type
                    || a.output() != *output
            }
        };
        if need_rebuild {
            let analyzer = SpectrumAnalyzer::new(*window_size, *window_type, *output, *sample_rate);
            analyzers.insert(sink_id.clone(), analyzer);
            log::debug!(
                "频谱分析器已 (重新)创建: sink={} window={} output={} fs={}",
                sink_id,
                window_size,
                match output {
                    vofa_next_dsp::SpectrumOutput::Magnitude => "Magnitude",
                    vofa_next_dsp::SpectrumOutput::Power => "Power",
                    vofa_next_dsp::SpectrumOutput::PSD => "PSD",
                    vofa_next_dsp::SpectrumOutput::Decibel => "Decibel",
                },
                sample_rate
            );
        }
    }
}

/// 同步 Ifft 节点重建缓冲与 graphs 中的 Ifft 节点
///
/// - 遍历所有 graph 的 Ifft 节点, 编译期解析其上游 FFT (SpectrumSink) 源 id
/// - 删除 graphs 中已不存在的 Ifft 节点状态
/// - 对每个有源 FFT 的 Ifft 节点, 读取最新频谱 (spectrum_snapshot[source_id]),
///   用 IfftSynth 合成时域缓冲并复位播放位置
///
/// 由 spectrum_ticker 每 tick 调用, 保证 Ifft 重建缓冲与图拓扑/最新频谱一致。
pub fn sync_ifft_buffers(state: &GraphEvalState) {
    let graphs = state.graphs.lock();
    let snapshot = state.spectrum_snapshot.lock();
    let mut ifft_states = state.ifft_states.lock();

    // 收集当前 Ifft 节点: id → 源 FFT (SpectrumSink) id + window_size
    let mut current: HashMap<String, Option<(String, usize)>> = HashMap::new();
    for (_, graph) in graphs.iter() {
        for node_id in graph.ifft_node_ids() {
            let cfg = graph
                .ifft_source(&node_id)
                .and_then(|sid| graph.spectrum_sink_config(&sid).map(|(n, _, _, _)| (sid, n)));
            current.insert(node_id, cfg);
        }
    }

    // 删除已不存在的 Ifft 节点状态
    ifft_states.retain(|id, _| current.contains_key(id));

    for (node_id, cfg) in &current {
        let entry = ifft_states.entry(node_id.clone()).or_default();
        match cfg {
            Some((sid, n)) => {
                if let Some(result) = snapshot.get(sid) {
                    entry.synth(&result.values, *n);
                }
            }
            None => {
                // 无上游 FFT 源: 清空缓冲, 输出 0
                entry.clear();
            }
        }
    }
}
