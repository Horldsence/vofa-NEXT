//! CAN 帧表格 — 最近 500 帧 (egui_extras::Table)
//!
//! 列: 时间戳 (µs) / ID (hex) / 扩展帧 / 方向 / DLC / 数据 (hex)。
//! 最新帧显示在最上方。

use std::sync::Arc;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use vofa_next_core::CanDirection;

use crate::core::AppState;

/// 表格显示的帧数上限
const MAX_FRAMES: usize = 500;

/// 渲染 CAN Data 页签
pub fn show(ui: &mut egui::Ui, state: &Arc<AppState>) {
    let frames = state.can_buffer.lock().get_recent(MAX_FRAMES);

    ui.horizontal(|ui| {
        ui.label(format!("{} frames (latest first)", frames.len()));
        if ui.button("Clear").clicked() {
            state.can_buffer.lock().clear();
        }
    });
    ui.separator();

    if frames.is_empty() {
        ui.label("No CAN frames received yet.");
        return;
    }

    // 最新在前
    let newest_first: Vec<_> = frames.iter().rev().collect();

    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(110.0).at_least(70.0))
        .column(Column::initial(80.0).at_least(50.0))
        .column(Column::initial(50.0).at_least(30.0))
        .column(Column::initial(50.0).at_least(30.0))
        .column(Column::initial(40.0).at_least(25.0))
        .column(Column::remainder())
        .min_scrolled_height(0.0)
        .header(20.0, |mut header| {
            header.col(|ui| {
                ui.strong("Time (µs)");
            });
            header.col(|ui| {
                ui.strong("ID");
            });
            header.col(|ui| {
                ui.strong("Ext");
            });
            header.col(|ui| {
                ui.strong("Dir");
            });
            header.col(|ui| {
                ui.strong("DLC");
            });
            header.col(|ui| {
                ui.strong("Data");
            });
        })
        .body(|body| {
            body.rows(18.0, newest_first.len(), |mut row| {
                let frame = newest_first[row.index()];
                row.col(|ui| {
                    ui.monospace(frame.timestamp.to_string());
                });
                row.col(|ui| {
                    let id_text = if frame.extended {
                        format!("{:08X}", frame.id)
                    } else {
                        format!("{:03X}", frame.id)
                    };
                    ui.monospace(id_text);
                });
                row.col(|ui| {
                    ui.monospace(if frame.extended { "EXT" } else { "STD" });
                });
                row.col(|ui| {
                    ui.monospace(match frame.direction {
                        CanDirection::Rx => "RX",
                        CanDirection::Tx => "TX",
                    });
                });
                row.col(|ui| {
                    ui.monospace(frame.dlc.to_string());
                });
                row.col(|ui| {
                    let data: Vec<String> = frame.data.iter().map(|b| format!("{b:02X}")).collect();
                    ui.monospace(data.join(" "));
                });
            });
        });

    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(100));
}
