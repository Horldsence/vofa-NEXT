use crate::core::state::GraphEvalState;
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
    let mut current_configs: HashMap<String, (usize, vofa_next_dsp::WindowType, vofa_next_dsp::SpectrumOutput, f32)> = HashMap::new();
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
            let analyzer = SpectrumAnalyzer::new(
                *window_size,
                *window_type,
                *output,
                *sample_rate,
            );
            analyzers.insert(sink_id.clone(), analyzer);
            tracing::info!(
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
