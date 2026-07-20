//! 顶部菜单栏

use eframe::egui;

use crate::ui::dock::DataKind;

/// 菜单栏一次渲染中用户触发的动作请求
#[derive(Debug, Default, Clone, Copy)]
pub struct MenuActions {
    pub open_settings: bool,
    pub new_tab: bool,
    pub close_tab: bool,
    pub toggle_sidebar: bool,
    /// 请求新建一个 Data 页签 (指定数据视图类型)
    pub add_data_view: Option<DataKind>,
}

/// 渲染顶部菜单栏, 返回本帧的动作请求
pub fn menu_bar(ui: &mut egui::Ui) -> MenuActions {
    let mut actions = MenuActions::default();

    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("VOFA-Next", |ui| {
            // TODO(Phase 3+): About 弹窗
            let _ = ui.button("About VOFA-Next");
            if ui.button("Settings…").clicked() {
                actions.open_settings = true;
                ui.close();
            }
        });

        ui.menu_button("File", |ui| {
            if ui
                .add(egui::Button::new("New Tab").shortcut_text("Ctrl/Cmd+T"))
                .clicked()
            {
                actions.new_tab = true;
                ui.close();
            }
            if ui
                .add(egui::Button::new("Close Tab").shortcut_text("Ctrl/Cmd+W"))
                .clicked()
            {
                actions.close_tab = true;
                ui.close();
            }
        });

        ui.menu_button("View", |ui| {
            if ui
                .add(egui::Button::new("Toggle Sidebar").shortcut_text("Ctrl/Cmd+B"))
                .clicked()
            {
                actions.toggle_sidebar = true;
                ui.close();
            }
            ui.menu_button("Add Data View", |ui| {
                for kind in DataKind::ALL {
                    if ui.button(kind.label()).clicked() {
                        actions.add_data_view = Some(kind);
                        ui.close();
                    }
                }
            });
        });

        ui.menu_button("Help", |ui| {
            if ui.button("Documentation").clicked() {
                ui.ctx()
                    .open_url(egui::OpenUrl::new_tab("https://vofa.plus"));
                ui.close();
            }
            if ui.button("GitHub").clicked() {
                ui.ctx()
                    .open_url(egui::OpenUrl::new_tab("https://github.com/vfaplus"));
                ui.close();
            }
        });
    });

    actions
}
