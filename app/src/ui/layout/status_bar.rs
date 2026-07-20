//! 底部状态栏 — 每帧直接读取 core::AppState 中的共享状态

use eframe::egui;
use vofa_next_core::ConnectionState;

use crate::core::AppState;

/// 渲染底部状态栏: 连接状态 + RX/TX 统计
pub fn status_bar(ui: &mut egui::Ui, state: &AppState) {
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        connection_indicator(ui, *state.connection_state.lock());

        ui.separator();

        let stats = state.stats.lock();
        ui.monospace(format!(
            "RX: {} B · {} frames",
            stats.rx_bytes, stats.rx_frames
        ));
        ui.separator();
        ui.monospace(format!(
            "TX: {} B · {} frames",
            stats.tx_bytes, stats.tx_frames
        ));
    });
}

fn connection_indicator(ui: &mut egui::Ui, state: ConnectionState) {
    let visuals = ui.visuals();
    let (label, color) = match state {
        ConnectionState::Disconnected => ("Disconnected", visuals.weak_text_color()),
        ConnectionState::Connecting => ("Connecting…", visuals.warn_fg_color),
        ConnectionState::Connected => ("Connected", crate::theme::success(visuals.dark_mode)),
        ConnectionState::Error => ("Error", visuals.error_fg_color),
    };
    ui.colored_label(color, "●");
    ui.label(label);
}
