//! 波形显示 — 多通道时域曲线 (egui_plot)
//!
//! 每帧从 `state.buffer` 读取最近 N 点并绘制折线。
//! 支持 Run/Stop、通道可见性勾选与自动缩放。

use std::sync::Arc;

use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};

use crate::core::AppState;

/// 单次读取的最大点数
const MAX_POINTS: usize = 2048;

/// 波形页签的持久化状态
pub struct WaveformState {
    /// true = 持续刷新最新数据; false = 冻结画面
    running: bool,
    /// 每通道可见性 (长度随检测到的通道数动态调整)
    channel_visible: Vec<bool>,
    /// 自动缩放 Y 轴
    auto_scale: bool,
}

impl WaveformState {
    pub fn new() -> Self {
        Self {
            running: true,
            channel_visible: Vec::new(),
            auto_scale: true,
        }
    }
}

impl Default for WaveformState {
    fn default() -> Self {
        Self::new()
    }
}

/// 通道曲线调色板
const CHANNEL_COLORS: [egui::Color32; 8] = [
    egui::Color32::from_rgb(0x4C, 0xAF, 0x50),
    egui::Color32::from_rgb(0x21, 0x96, 0xF3),
    egui::Color32::from_rgb(0xFF, 0x98, 0x00),
    egui::Color32::from_rgb(0xE9, 0x1E, 0x63),
    egui::Color32::from_rgb(0x9C, 0x27, 0xB0),
    egui::Color32::from_rgb(0x00, 0xBC, 0xD4),
    egui::Color32::from_rgb(0xFF, 0xEB, 0x3B),
    egui::Color32::from_rgb(0xFF, 0x57, 0x22),
];

/// 渲染波形 Data 页签
pub fn show(ui: &mut egui::Ui, state: &Arc<AppState>, view: &mut WaveformState) {
    // 冻结时不读取新数据? 仍需读一次以绘制当前缓冲, 简化起见每帧都读,
    // running=false 时只是不再自动滚动 (曲线仍反映缓冲快照)。
    let window = state.buffer.lock().get_recent(MAX_POINTS);

    // 通道数变化时扩展可见性列表 (默认可见)
    let ch_count = window.channel_count.max(window.channels.len());
    while view.channel_visible.len() < ch_count {
        view.channel_visible.push(true);
    }

    ui.horizontal(|ui| {
        let toggle_label = if view.running { "⏸ Stop" } else { "▶ Run" };
        if ui.button(toggle_label).clicked() {
            view.running = !view.running;
        }
        ui.checkbox(&mut view.auto_scale, "Auto-scale");
        ui.separator();
        ui.label(format!("channels: {ch_count}"));
        ui.label(format!("points: {}", window.timestamps.len()));
    });

    ui.horizontal_wrapped(|ui| {
        for ch in 0..ch_count {
            let mut visible = view.channel_visible[ch];
            let color = CHANNEL_COLORS[ch % CHANNEL_COLORS.len()];
            let text = egui::RichText::new(format!("CH{ch}")).color(color);
            if ui.checkbox(&mut visible, text).changed() {
                view.channel_visible[ch] = visible;
            }
        }
    });
    ui.separator();

    let mut plot = Plot::new("waveform_plot")
        .legend(egui_plot::Legend::default())
        .x_axis_label("t (s, relative)")
        .allow_zoom(true)
        .allow_drag(true)
        .allow_scroll(true);
    if view.auto_scale {
        plot = plot.auto_bounds(egui::Vec2b::new(true, true));
    } else {
        plot = plot.auto_bounds(egui::Vec2b::new(true, false));
    }

    plot.show(ui, |plot_ui| {
        for (ch, data) in window.channels.iter().enumerate() {
            if ch >= ch_count || !view.channel_visible[ch] {
                continue;
            }
            let points: Vec<[f64; 2]> = window
                .timestamps
                .iter()
                .zip(data.iter())
                .map(|(&t_ms, &v)| [t_ms as f64 / 1000.0, v as f64])
                .collect();
            if points.is_empty() {
                continue;
            }
            let line = Line::new(format!("CH{ch}"), PlotPoints::from(points))
                .color(CHANNEL_COLORS[ch % CHANNEL_COLORS.len()]);
            plot_ui.line(line);
        }
    });

    if view.running {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
}
