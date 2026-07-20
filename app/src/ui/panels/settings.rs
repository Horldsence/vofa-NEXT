//! 应用设置面板 — 外观 / 通用 / 数据容量

use std::sync::Arc;

use eframe::egui;
use parking_lot::Mutex;

use crate::core::{services, AppState};
use crate::settings::AppSettings;
use crate::theme::{self, ThemeKind};

const LANGUAGES: [(&str, &str); 2] = [("zh-CN", "简体中文"), ("en-US", "English (US)")];

pub struct SettingsPanel {
    /// 可编辑的设置副本 (面板内唯一权威, VofaApp 启动时注入)
    settings: AppSettings,
    /// 最近一次异步操作的结果提示 (错误信息)
    status: Arc<Mutex<Option<String>>>,
}

impl Default for SettingsPanel {
    fn default() -> Self {
        Self::new(AppSettings::default())
    }
}

impl SettingsPanel {
    pub fn new(settings: AppSettings) -> Self {
        Self {
            settings,
            status: Arc::new(Mutex::new(None)),
        }
    }

    /// 是否需要在启动时显示新手引导
    pub fn show_onboarding(&self) -> bool {
        self.settings.show_onboarding
    }

    /// 当前主题值 (供 VofaApp 启动时应用)
    pub fn theme(&self) -> ThemeKind {
        self.settings.theme
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, state: &Arc<AppState>, rt: &tokio::runtime::Runtime) {
        // ---- 外观 ----
        ui.label("Appearance");
        let prev_theme = self.settings.theme;
        egui::ComboBox::from_label("Theme")
            .selected_text(self.settings.theme.label())
            .show_ui(ui, |ui| {
                for kind in ThemeKind::ALL {
                    ui.selectable_value(&mut self.settings.theme, kind, kind.label());
                }
            });
        // 主题变更: 立即生效并持久化
        if self.settings.theme != prev_theme {
            theme::apply(ui.ctx(), self.settings.theme);
            self.settings.save();
        }
        ui.add_space(8.0);

        // ---- 通用 ----
        ui.label("General");
        if ui
            .checkbox(
                &mut self.settings.show_onboarding,
                "Show onboarding at startup",
            )
            .changed()
        {
            self.settings.save();
        }
        let prev_language = self.settings.language.clone();
        egui::ComboBox::from_label("Language")
            .selected_text(language_label(&self.settings.language))
            .show_ui(ui, |ui| {
                for (value, label) in LANGUAGES {
                    ui.selectable_value(&mut self.settings.language, value.to_string(), label);
                }
            });
        // 语言仅存储, 完整 i18n 后续接入
        if self.settings.language != prev_language {
            self.settings.save();
        }
        ui.add_space(8.0);

        // ---- 数据容量 ----
        ui.label("Data Capacity");
        capacity_row(
            ui,
            "Waveform points",
            &mut self.settings.waveform_buffer_capacity,
        );
        capacity_row(
            ui,
            "Raw data bytes",
            &mut self.settings.rawdata_buffer_capacity,
        );
        capacity_row(ui, "CAN frames", &mut self.settings.can_buffer_capacity);
        capacity_row(
            ui,
            "Logic samples",
            &mut self.settings.logic_buffer_capacity,
        );
        ui.add_space(4.0);

        if ui.button("Apply").clicked() {
            let state = state.clone();
            let status = self.status.clone();
            let (wf, raw, can, logic) = (
                self.settings.waveform_buffer_capacity,
                self.settings.rawdata_buffer_capacity,
                self.settings.can_buffer_capacity,
                self.settings.logic_buffer_capacity,
            );
            self.settings.save();
            rt.spawn(async move {
                let result = async {
                    services::set_waveform_buffer_capacity(&state, wf).await?;
                    services::set_rawdata_buffer_capacity(&state, raw).await?;
                    services::set_can_buffer_capacity(&state, can).await?;
                    services::set_logic_buffer_capacity(&state, logic).await
                }
                .await;
                *status.lock() = result.err().map(|e| e.to_string());
            });
        }

        if let Some(msg) = self.status.lock().as_ref() {
            ui.add_space(4.0);
            ui.colored_label(ui.visuals().error_fg_color, msg);
        }
    }
}

fn language_label(value: &str) -> &'static str {
    LANGUAGES
        .iter()
        .find(|(v, _)| *v == value)
        .map(|(_, l)| *l)
        .unwrap_or("简体中文")
}

fn capacity_row(ui: &mut egui::Ui, label: &str, value: &mut usize) {
    ui.horizontal(|ui| {
        ui.label(format!("{label}:"));
        ui.add(egui::DragValue::new(value).range(1..=10_000_000));
    });
}
