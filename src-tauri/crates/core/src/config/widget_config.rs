//! 控件配置
//!
//! 涵盖前端 widget (控件/显示) 的配置模型:
//! - [`WidgetConfig`] tagged enum
//! - 操作控件: [`KnobConfig`] / [`ButtonConfig`] / [`RadioConfig`] / [`CheckboxConfig`] / [`SliderConfig`] / [`LabelConfig`]
//! - 显示控件: [`WaveformConfig`] / [`PieChartConfig`] / [`ImageConfig`] + [`ImageFormat`]
//! - [`WidgetBinding`] 数据绑定模式 (None/Auto/Manual)

use serde::{Deserialize, Serialize};

/// 控件类型 — tagged enum, 序列化 `{ kind, params }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "params")]
pub enum WidgetConfig {
    // 操作控件
    Knob(KnobConfig),
    Button(ButtonConfig),
    Radio(RadioConfig),
    Checkbox(CheckboxConfig),
    Slider(SliderConfig),
    Label(LabelConfig),
    // 显示控件
    Waveform(WaveformConfig),
    PieChart(PieChartConfig),
    Image(ImageConfig),
}

/// 旋钮控件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnobConfig {
    pub id: String,
    pub label: String,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub default: f32,
    /// 绑定模式
    pub binding: WidgetBinding,
}

/// 按钮控件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonConfig {
    pub id: String,
    pub label: String,
    pub press_value: f32,
    pub release_value: f32,
    pub binding: WidgetBinding,
}

/// 单选控件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioConfig {
    pub id: String,
    pub label: String,
    pub options: Vec<(String, f32)>,
    pub default: usize,
    pub binding: WidgetBinding,
}

/// 复选框控件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckboxConfig {
    pub id: String,
    pub label: String,
    pub checked_value: f32,
    pub unchecked_value: f32,
    pub default: bool,
    pub binding: WidgetBinding,
}

/// 滑动条控件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliderConfig {
    pub id: String,
    pub label: String,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub default: f32,
    pub binding: WidgetBinding,
}

/// 文本标签控件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelConfig {
    pub id: String,
    pub text: String,
    /// 绑定到接收通道 (可选)
    pub channel: Option<usize>,
}

/// 波形显示控件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformConfig {
    pub id: String,
    pub channels: usize,
    /// 每通道最大点数
    pub max_points: usize,
    /// 显示通道列表
    pub visible_channels: Vec<bool>,
}

/// 饼图控件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PieChartConfig {
    pub id: String,
    pub label: String,
    /// 扇区标签
    pub segments: Vec<String>,
    /// 绑定到接收通道
    pub channels: Vec<usize>,
}

/// 图像控件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageConfig {
    pub id: String,
    pub label: String,
    /// 图像宽度
    pub width: u32,
    pub height: u32,
    /// 像素格式
    pub format: ImageFormat,
}

/// 图像像素格式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Rgb888,
    Rgb565,
    Gray8,
}

/// 控件数据绑定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", content = "params")]
pub enum WidgetBinding {
    /// 不绑定
    None,
    /// 自动绑定到 VOFA 通道
    Auto { channel: usize },
    /// 手动命令模板, {value} 会被替换
    Manual { template: String },
}