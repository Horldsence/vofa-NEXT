//! 左侧活动栏 (VS Code 风格图标列)

use eframe::egui;

/// 活动栏可选面板
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityItem {
    Transport,
    Protocol,
    Widgets,
    Settings,
    About,
    Help,
}

impl ActivityItem {
    pub const ALL: [Self; 6] = [
        Self::Transport,
        Self::Protocol,
        Self::Widgets,
        Self::Settings,
        Self::About,
        Self::Help,
    ];

    pub fn icon(self) -> &'static str {
        match self {
            Self::Transport => "⚡",
            Self::Protocol => "◉",
            Self::Widgets => "◈",
            Self::Settings => "⚙",
            Self::About => "ⓘ",
            Self::Help => "?",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Transport => "Transport",
            Self::Protocol => "Protocol",
            Self::Widgets => "Widgets",
            Self::Settings => "Settings",
            Self::About => "About",
            Self::Help => "Help",
        }
    }
}

/// 渲染活动栏。点击已选中项会折叠/展开侧栏, 点击其他项切换面板并展开侧栏。
pub fn activity_bar(ui: &mut egui::Ui, active: &mut ActivityItem, sidebar_visible: &mut bool) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            for item in ActivityItem::ALL {
                let selected = *active == item && *sidebar_visible;
                let response = ui
                    .add(egui::Button::selectable(
                        selected,
                        egui::RichText::new(item.icon()).size(18.0),
                    ))
                    .on_hover_text(item.label());
                if response.clicked() {
                    if *active == item {
                        *sidebar_visible = !*sidebar_visible;
                    } else {
                        *active = item;
                        *sidebar_visible = true;
                    }
                }
                ui.add_space(4.0);
            }
        });
    });
}
