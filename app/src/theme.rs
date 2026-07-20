//! Catppuccin 主题 — 自实现的 egui 配色/样式 (catppuccin-egui 尚不支持 egui 0.34)
//!
//! 提供四种官方风味 (Mocha / Macchiato / Frappé / Latte) 的调色板,
//! 并统一设置 [`egui::Visuals`] 与 [`egui::Style`] (圆角 / 边框 / 间距)。

use egui::{Color32, CornerRadius, Margin, Stroke};

/// Catppuccin 主题风味
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeKind {
    /// 深色 (默认)
    #[default]
    Mocha,
    Macchiato,
    Frappé,
    /// 浅色
    Latte,
}

impl ThemeKind {
    pub const ALL: [Self; 4] = [Self::Mocha, Self::Macchiato, Self::Frappé, Self::Latte];

    /// 持久化字符串形式
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mocha => "mocha",
            Self::Macchiato => "macchiato",
            Self::Frappé => "frappe",
            Self::Latte => "latte",
        }
    }

    /// 宽松解析; 兼容旧版设置值 ("system"/"dark" → Mocha, "light" → Latte)
    pub fn from_str_lossy(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "macchiato" => Self::Macchiato,
            "frappe" | "frappé" => Self::Frappé,
            "latte" | "light" => Self::Latte,
            // "mocha" / "system" / "dark" / 未知值
            _ => Self::Mocha,
        }
    }

    /// 下拉框显示名
    pub fn label(self) -> &'static str {
        match self {
            Self::Mocha => "Catppuccin Mocha",
            Self::Macchiato => "Catppuccin Macchiato",
            Self::Frappé => "Catppuccin Frappé",
            Self::Latte => "Catppuccin Latte",
        }
    }

    pub fn is_dark(self) -> bool {
        !matches!(self, Self::Latte)
    }

    fn palette(self) -> Palette {
        match self {
            Self::Mocha => MOCHA,
            Self::Macchiato => MACCHIATO,
            Self::Frappé => FRAPPE,
            Self::Latte => LATTE,
        }
    }
}

impl serde::Serialize for ThemeKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for ThemeKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_str_lossy(&value))
    }
}

// ---------------------------------------------------------------------------
// Catppuccin 调色板 (官方 hex 值)
// ---------------------------------------------------------------------------

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(((hex >> 16) & 0xff) as u8, ((hex >> 8) & 0xff) as u8, (hex & 0xff) as u8)
}

/// 主题用到的 Catppuccin 颜色子集
#[derive(Debug, Clone, Copy)]
struct Palette {
    base: Color32,
    mantle: Color32,
    crust: Color32,
    text: Color32,
    subtext0: Color32,
    #[allow(dead_code)]
    subtext1: Color32,
    surface0: Color32,
    surface1: Color32,
    surface2: Color32,
    overlay0: Color32,
    #[allow(dead_code)]
    overlay1: Color32,
    overlay2: Color32,
    blue: Color32,
    green: Color32,
    red: Color32,
    yellow: Color32,
    #[allow(dead_code)]
    peach: Color32,
    #[allow(dead_code)]
    mauve: Color32,
    #[allow(dead_code)]
    teal: Color32,
}

const MOCHA: Palette = Palette {
    base: rgb(0x1e1e2e),
    mantle: rgb(0x181825),
    crust: rgb(0x11111b),
    text: rgb(0xcdd6f4),
    subtext0: rgb(0xa6adc8),
    subtext1: rgb(0xbac2de),
    surface0: rgb(0x313244),
    surface1: rgb(0x45475a),
    surface2: rgb(0x585b70),
    overlay0: rgb(0x6c7086),
    overlay1: rgb(0x7f849c),
    overlay2: rgb(0x9399b2),
    blue: rgb(0x89b4fa),
    green: rgb(0xa6e3a1),
    red: rgb(0xf38ba8),
    yellow: rgb(0xf9e2af),
    peach: rgb(0xfab387),
    mauve: rgb(0xcba6f7),
    teal: rgb(0x94e2d5),
};

const MACCHIATO: Palette = Palette {
    base: rgb(0x24273a),
    mantle: rgb(0x1e2030),
    crust: rgb(0x181926),
    text: rgb(0xcad3f5),
    subtext0: rgb(0xa5adcb),
    subtext1: rgb(0xb8c0e0),
    surface0: rgb(0x363a4f),
    surface1: rgb(0x494d64),
    surface2: rgb(0x5b6078),
    overlay0: rgb(0x6e738d),
    overlay1: rgb(0x8087a2),
    overlay2: rgb(0x939ab7),
    blue: rgb(0x8aadf4),
    green: rgb(0xa6da95),
    red: rgb(0xed8796),
    yellow: rgb(0xeed49f),
    peach: rgb(0xf5a97f),
    mauve: rgb(0xc6a0f6),
    teal: rgb(0x8bd5ca),
};

const FRAPPE: Palette = Palette {
    base: rgb(0x303446),
    mantle: rgb(0x292c3c),
    crust: rgb(0x232634),
    text: rgb(0xc6d0f5),
    subtext0: rgb(0xa5adce),
    subtext1: rgb(0xb5bfe2),
    surface0: rgb(0x414559),
    surface1: rgb(0x51576d),
    surface2: rgb(0x626880),
    overlay0: rgb(0x737994),
    overlay1: rgb(0x838ba7),
    overlay2: rgb(0x949cbb),
    blue: rgb(0x8caaee),
    green: rgb(0xa6d189),
    red: rgb(0xe78284),
    yellow: rgb(0xe5c890),
    peach: rgb(0xef9f76),
    mauve: rgb(0xca9ee6),
    teal: rgb(0x81c8be),
};

const LATTE: Palette = Palette {
    base: rgb(0xeff1f5),
    mantle: rgb(0xe6e9ef),
    crust: rgb(0xdce0e8),
    text: rgb(0x4c4f69),
    subtext0: rgb(0x6c6f85),
    subtext1: rgb(0x5c5f77),
    surface0: rgb(0xccd0da),
    surface1: rgb(0xbcc0cc),
    surface2: rgb(0xacb0be),
    overlay0: rgb(0x9ca0b0),
    overlay1: rgb(0x8c8fa1),
    overlay2: rgb(0x7c7f93),
    blue: rgb(0x1e66f5),
    green: rgb(0x40a02b),
    red: rgb(0xd20f39),
    yellow: rgb(0xdf8e1d),
    peach: rgb(0xfe640b),
    mauve: rgb(0x8839ef),
    teal: rgb(0x179299),
};

// ---------------------------------------------------------------------------
// 应用主题
// ---------------------------------------------------------------------------

/// 将指定 Catppuccin 风味应用到 egui 上下文 (Visuals + Style)
pub fn apply(ctx: &egui::Context, kind: ThemeKind) {
    let p = kind.palette();
    let dark = kind.is_dark();
    let radius = CornerRadius::same(6);

    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.dark_mode = dark;

    // 背景层次
    visuals.panel_fill = p.base;
    visuals.window_fill = p.mantle;
    visuals.extreme_bg_color = p.crust;
    visuals.faint_bg_color = p.surface0;
    visuals.code_bg_color = p.mantle;

    // 文本 / 链接 / 选区
    visuals.override_text_color = None;
    visuals.hyperlink_color = p.blue;
    visuals.selection.bg_fill = if dark {
        p.blue.gamma_multiply(0.35)
    } else {
        p.blue.gamma_multiply(0.25)
    };
    visuals.selection.stroke = Stroke::new(1.0, p.blue);

    // 窗口边框
    visuals.window_stroke = Stroke::new(1.0, p.surface1);

    let text_stroke = Stroke::new(1.0, p.text);
    let subtext_stroke = Stroke::new(1.0, p.subtext0);
    let border = Stroke::new(1.0, p.surface1);

    // 非交互元素 (标签/面板背景)
    visuals.widgets.noninteractive.bg_fill = p.base;
    visuals.widgets.noninteractive.weak_bg_fill = p.base;
    visuals.widgets.noninteractive.bg_stroke = border;
    visuals.widgets.noninteractive.fg_stroke = text_stroke;
    visuals.widgets.noninteractive.corner_radius = radius;

    // 静止可交互控件
    visuals.widgets.inactive.bg_fill = p.surface0;
    visuals.widgets.inactive.weak_bg_fill = p.surface0;
    visuals.widgets.inactive.bg_stroke = border;
    visuals.widgets.inactive.fg_stroke = text_stroke;
    visuals.widgets.inactive.corner_radius = radius;

    // 悬停
    visuals.widgets.hovered.bg_fill = p.surface1;
    visuals.widgets.hovered.weak_bg_fill = p.surface1;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, p.overlay0);
    visuals.widgets.hovered.fg_stroke = text_stroke;
    visuals.widgets.hovered.corner_radius = radius;

    // 按下/激活
    visuals.widgets.active.bg_fill = p.surface2;
    visuals.widgets.active.weak_bg_fill = p.surface2;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, p.overlay2);
    visuals.widgets.active.fg_stroke = text_stroke;
    visuals.widgets.active.corner_radius = radius;

    // 展开 (ComboBox 等)
    visuals.widgets.open.bg_fill = p.surface0;
    visuals.widgets.open.weak_bg_fill = p.surface0;
    visuals.widgets.open.bg_stroke = border;
    visuals.widgets.open.fg_stroke = subtext_stroke;
    visuals.widgets.open.corner_radius = radius;

    // 语义色 (供强提示处取用)
    visuals.warn_fg_color = p.yellow;
    visuals.error_fg_color = p.red;

    ctx.global_style_mut(|style| {
        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(8.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
        style.spacing.window_margin = Margin::same(10);
        style.spacing.menu_margin = Margin::same(6);
        style.spacing.menu_spacing = 2.0;
    });
}

// ---------------------------------------------------------------------------
// 供 UI 取用的强调色
// ---------------------------------------------------------------------------

/// 成功色 (按深/浅色选择对应风味; 深色风味间 green 差异可忽略)
pub fn success(dark: bool) -> Color32 {
    if dark { MOCHA.green } else { LATTE.green }
}
