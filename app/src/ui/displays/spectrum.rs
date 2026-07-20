//! 频谱显示 — FFT 幅度谱 (egui_plot)
//!
//! 数据源为 `state.spectrum_snapshot` (SpectrumSink widget id → SpectrumResult)。
//! `SpectrumResult` 仅含 (频率, 幅度值) 配对, 无相位信息, 故只绘制幅度谱。

use std::sync::Arc;

use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use vofa_next_dsp::SpectrumResult;

use crate::core::AppState;

/// 频谱页签的持久化状态
pub struct SpectrumState {
    /// 当前选中的 SpectrumSink widget id (None = 自动选第一个)
    selected: Option<String>,
}

impl SpectrumState {
    pub fn new() -> Self {
        Self { selected: None }
    }
}

impl Default for SpectrumState {
    fn default() -> Self {
        Self::new()
    }
}

/// 渲染频谱 Data 页签
pub fn show(ui: &mut egui::Ui, state: &Arc<AppState>, view: &mut SpectrumState) {
    let snapshot: Vec<(String, SpectrumResult)> = state
        .spectrum_snapshot
        .lock()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    if snapshot.is_empty() {
        ui.label("No spectrum data yet. Add a Spectrum sink node and feed data to it.");
        return;
    }

    // 校验选中项仍有效, 否则回退到第一个
    let selected = match &view.selected {
        Some(sel) if snapshot.iter().any(|(k, _)| k == sel) => sel.clone(),
        _ => snapshot[0].0.clone(),
    };
    view.selected = Some(selected.clone());

    ui.horizontal(|ui| {
        ui.label("Sink:");
        for (key, _) in &snapshot {
            if ui.selectable_label(*key == selected, key).clicked() {
                view.selected = Some(key.clone());
            }
        }
    });
    ui.separator();

    if let Some((_, result)) = snapshot.iter().find(|(k, _)| *k == selected) {
        let points: Vec<[f64; 2]> = result
            .frequencies
            .iter()
            .zip(result.values.iter())
            .map(|(&f, &v)| [f as f64, v as f64])
            .collect();

        let peak = result
            .values
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        ui.label(format!("bins: {}  peak: {peak:.4}", points.len()));

        Plot::new("spectrum_plot")
            .x_axis_label("Frequency (Hz)")
            .y_axis_label("Magnitude")
            .auto_bounds(egui::Vec2b::new(true, true))
            .allow_zoom(true)
            .allow_drag(true)
            .allow_scroll(true)
            .show(ui, |plot_ui| {
                plot_ui.line(
                    Line::new("magnitude", PlotPoints::from(points))
                        .color(egui::Color32::from_rgb(0x21, 0x96, 0xF3)),
                );
            });
    }

    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(100));
}
