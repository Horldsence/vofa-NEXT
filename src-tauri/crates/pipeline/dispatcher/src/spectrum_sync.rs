use data_plane::GraphEvalState;
use dsp_fft::SpectrumAnalyzer;
use std::collections::HashMap;

/// 同步 spectrum_analyzers 与 graphs 中的 Fft 节点
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

    // 收集所有 graph 中当前的 Fft id → config
    let mut current_configs: HashMap<
        String,
        (
            usize,
            usize,
            dsp_window::WindowType,
            dsp_fft::SpectrumOutput,
            f32,
        ),
    > = HashMap::new();
    for (_, graph) in graphs.iter() {
        for sink_id in graph.spectrum_sink_ids() {
            if let Some(cfg) = graph.spectrum_sink_config(sink_id) {
                current_configs.insert(sink_id.clone(), cfg);
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
    for (sink_id, (window_size, hop_size, window_type, output, sample_rate)) in &current_configs {
        // 任一配置变化都需要重建 (window_size/sample_rate 需要 new FFT planner;
        // window_type/output 虽有 setter 但重建更简单且不影响性能)
        let need_rebuild = !analyzers.get(sink_id).is_some_and(|a| {
            a.window_size() == *window_size
                && a.hop_size() == *hop_size
                && (a.sample_rate() - *sample_rate).abs() < f32::EPSILON
                && a.window_type() == *window_type
                && a.output() == *output
        });
        if need_rebuild {
            let Ok(analyzer) = SpectrumAnalyzer::with_config(
                dsp_fft::TransformConfig {
                    window_size: *window_size,
                    hop_size: *hop_size,
                    window_type: *window_type,
                    sample_rate: *sample_rate,
                },
                *output,
            ) else {
                continue;
            };
            analyzers.insert(sink_id.clone(), analyzer);
            log::debug!(
                "频谱分析器已 (重新)创建: sink={} window={} output={} fs={}",
                sink_id,
                window_size,
                match output {
                    dsp_fft::SpectrumOutput::Magnitude => "Magnitude",
                    dsp_fft::SpectrumOutput::Power => "Power",
                    dsp_fft::SpectrumOutput::PSD => "PSD",
                    dsp_fft::SpectrumOutput::Decibel => "Decibel",
                },
                sample_rate
            );
        }
    }
}

/// Remove orphan reconstruction state. Complex frames are consumed in the sample loop.
pub fn sync_ifft_buffers(state: &GraphEvalState) {
    let graphs = state.graphs.lock();
    let current: std::collections::HashSet<&str> = graphs
        .values()
        .flat_map(|graph| graph.ifft_node_ids().iter().map(String::as_str))
        .collect();
    state
        .ifft_states
        .lock()
        .retain(|id, _| current.contains(id.as_str()));
}
