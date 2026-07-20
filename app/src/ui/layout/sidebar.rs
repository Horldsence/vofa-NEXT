//! 左侧可折叠/可拖拽侧栏 — 显示活动栏当前选中的面板

use std::sync::Arc;

use eframe::egui;

use super::activity_bar::ActivityItem;
use crate::core::AppState;
use crate::ui::panels::Panels;

/// 渲染左侧 SidePanel, 按活动栏选中项分发到真实面板。
pub fn sidebar(
    ui: &mut egui::Ui,
    visible: bool,
    panel: ActivityItem,
    panels: &mut Panels,
    state: &Arc<AppState>,
    rt: &Arc<tokio::runtime::Runtime>,
) {
    egui::Panel::left("sidebar")
        .resizable(true)
        .default_size(260.0)
        .size_range(180.0..=480.0)
        .show_animated_inside(ui, visible, |ui| {
            ui.add_space(4.0);
            ui.heading(panel.label());
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| match panel {
                ActivityItem::Transport => panels.transport.ui(ui, state, rt),
                ActivityItem::Protocol => panels.protocol.ui(ui, state, rt),
                ActivityItem::Widgets => panels.widget_palette.ui(ui),
                ActivityItem::Settings => panels.settings.ui(ui, state, rt),
                ActivityItem::About => {
                    ui.label("VOFA-Next");
                    ui.small("Cross-platform serial data debugging tool (egui edition).");
                }
                ActivityItem::Help => {
                    ui.label("Shortcuts:");
                    ui.monospace("Ctrl/Cmd+T  New tab");
                    ui.monospace("Ctrl/Cmd+W  Close tab");
                    ui.monospace("Ctrl/Cmd+B  Toggle sidebar");
                    ui.monospace("Ctrl/Cmd+,  Settings");
                }
            });
        });
}
