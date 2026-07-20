//! 应用内 Toast 通知 — 右下角浮层, 自动过期

use eframe::egui;

/// Toast 级别
///
/// `Warning` 预留给后续接线 (当前仅 Info/Success/Error 被使用)
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastLevel {
    fn color(self, visuals: &egui::Visuals) -> egui::Color32 {
        match self {
            Self::Info => visuals.hyperlink_color,
            Self::Success => crate::theme::success(visuals.dark_mode),
            Self::Warning => visuals.warn_fg_color,
            Self::Error => visuals.error_fg_color,
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Info => "ℹ",
            Self::Success => "✓",
            Self::Warning => "⚠",
            Self::Error => "✕",
        }
    }
}

/// 单条 Toast
#[derive(Debug, Clone)]
pub struct Toast {
    pub level: ToastLevel,
    pub text: String,
    /// 剩余显示时间 (秒)
    pub ttl_secs: f32,
}

/// Toast 队列 — 由 VofaApp 持有, 每帧渲染并递减 TTL
#[derive(Default)]
pub struct ToastQueue {
    toasts: Vec<Toast>,
}

impl ToastQueue {
    /// 推入一条 Toast (默认 4 秒)
    pub fn push(&mut self, level: ToastLevel, text: impl Into<String>) {
        self.toasts.push(Toast {
            level,
            text: text.into(),
            ttl_secs: 4.0,
        });
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.push(ToastLevel::Info, text);
    }

    pub fn success(&mut self, text: impl Into<String>) {
        self.push(ToastLevel::Success, text);
    }

    /// 预留给后续接线
    #[allow(dead_code)]
    pub fn warning(&mut self, text: impl Into<String>) {
        self.push(ToastLevel::Warning, text);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.push(ToastLevel::Error, text);
    }

    /// 渲染到屏幕右下角, 并按帧间隔递减 TTL
    pub fn render(&mut self, ctx: &egui::Context) {
        if self.toasts.is_empty() {
            return;
        }

        let dt = ctx.input(|i| i.stable_dt);
        for toast in &mut self.toasts {
            toast.ttl_secs -= dt;
        }
        self.toasts.retain(|t| t.ttl_secs > 0.0);
        // TTL 递减需要持续重绘
        ctx.request_repaint();

        egui::Area::new(egui::Id::new("toast_area"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    for toast in &self.toasts {
                        egui::Frame::window(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.colored_label(
                                    toast.level.color(ui.visuals()),
                                    toast.level.icon(),
                                );
                                ui.label(&toast.text);
                            });
                        });
                        ui.add_space(4.0);
                    }
                });
            });
    }
}
