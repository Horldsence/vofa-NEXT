//! 可复用的 egui 控件绘制 — 节点编辑器内嵌控件 (Phase 6)
//!
//! 提供 Number / Gauge / Led 显示控件的绘制函数, 供节点编辑器
//! (以及后续独立的控件页签) 复用。

use eframe::egui;

/// 大号数字读数
pub fn number_display(ui: &mut egui::Ui, value: f32) {
    ui.add(egui::Label::new(
        egui::RichText::new(format!("{value:.3}"))
            .monospace()
            .size(22.0)
            .strong(),
    ));
}

/// 进度条式表盘 — value 按 [min, max] 归一化后绘制
pub fn gauge(ui: &mut egui::Ui, value: f32, min: f32, max: f32, width: f32) {
    let frac = if max > min {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let bar = egui::ProgressBar::new(frac)
        .desired_width(width)
        .text(format!("{value:.2}"));
    ui.add(bar);
}

/// LED 指示灯 — value > threshold 点亮
pub fn led(ui: &mut egui::Ui, value: f32, threshold: f32, radius: f32) {
    let on = value > threshold;
    let (rect, _resp) = ui.allocate_exact_size(
        egui::vec2(radius * 2.0, radius * 2.0),
        egui::Sense::hover(),
    );
    let fill = if on {
        egui::Color32::from_rgb(0x4c, 0xc2, 0x61)
    } else {
        ui.visuals().widgets.inactive.bg_fill
    };
    ui.painter().circle(
        rect.center(),
        radius,
        fill,
        egui::Stroke::new(1.0, ui.visuals().widgets.inactive.fg_stroke.color),
    );
}
