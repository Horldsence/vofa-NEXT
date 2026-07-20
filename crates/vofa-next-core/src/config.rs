use serde::{Deserialize, Serialize};

use crate::can::CanBitrate;

// ============ 传输层配置 ============

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "params")]
pub enum TransportConfig {
    Serial(SerialConfig),
    Udp(UdpConfig),
    TcpClient(TcpClientConfig),
    TcpServer(TcpServerConfig),
    TestData(TestDataConfig),
    Slcan(SlcanConfig),
    CandleLight(CandleConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerialConfig {
    pub port_name: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub flow_control: FlowControl,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port_name: String::new(),
            baud_rate: 115200,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: StopBits::One,
            flow_control: FlowControl::None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Parity {
    None,
    Odd,
    Even,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StopBits {
    One,
    Two,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FlowControl {
    None,
    Software,
    Hardware,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpConfig {
    pub local_addr: String,
    pub remote_addr: String,
    pub local_port: u16,
    pub remote_port: u16,
}

impl Default for UdpConfig {
    fn default() -> Self {
        Self {
            local_addr: "0.0.0.0".into(),
            remote_addr: "127.0.0.1".into(),
            local_port: 0,
            remote_port: 8888,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpClientConfig {
    pub host: String,
    pub port: u16,
}

impl Default for TcpClientConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8888,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpServerConfig {
    pub listen_addr: String,
    pub listen_port: u16,
}

impl Default for TcpServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0".into(),
            listen_port: 8888,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestDataConfig {
    /// 通道数
    pub channels: usize,
    /// 采样率 Hz
    pub sample_rate: f32,
    /// 信号类型
    pub signal: TestSignal,
}

impl Default for TestDataConfig {
    fn default() -> Self {
        Self {
            channels: 4,
            sample_rate: 1000.0,
            signal: TestSignal::Sine,
        }
    }
}

/// slcan 配置 — 基于 USB-CDC 串口
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlcanConfig {
    pub port_name: String,
    pub baud_rate: u32, // 串口波特率 (通常 115200 或 1M)
    pub can_bitrate: CanBitrate,
}

impl Default for SlcanConfig {
    fn default() -> Self {
        Self {
            port_name: String::new(),
            baud_rate: 115200,
            can_bitrate: CanBitrate::Bps500k,
        }
    }
}

/// candleLight 配置 — 原生 USB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandleConfig {
    pub bus: u8,
    pub address: u8,
    pub can_bitrate: CanBitrate,
    pub channel: u8, // CAN 通道 (0/1)
}

impl Default for CandleConfig {
    fn default() -> Self {
        Self {
            bus: 0,
            address: 0,
            can_bitrate: CanBitrate::Bps500k,
            channel: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TestSignal {
    Sine,
    Square,
    Triangle,
    Sawtooth,
    Random,
    /// 直流 (固定值)
    Dc,
    /// 扫频信号
    Chirp,
    /// 阶梯信号
    Steps,
    /// 高斯噪声
    Noise,
    /// 多频叠加
    MultiTone,
}

// ============ 协议层配置 ============

/// 协议配置
/// channels: None = 自动检测通道数, Some(n) = 手动指定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ProtocolConfig {
    JustFloat { channels: Option<usize> },
    FireWater { channels: Option<usize> },
    RawData,
    Slcan,
    CandleLight,
    LogicDecode { decoder: crate::logic::LogicDecoderConfig },
    /// 诊断协议层 (ISO-TP / UDS / OBD-II / J1939)
    ///
    /// 注意:诊断流程走独立的 `DiagnosticEngine` + `BridgeCanBackend` 管线,
    /// 不通过 `ProtocolEngine` 的 feed/encode 通路。`create_engine` 对此变体
    /// 返回 `RawDataEngine` 占位,真正的诊断 dispatch 在 `state.rs` 中实现。
    Diagnostic { config: crate::diagnostic::DiagnosticConfig },
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self::JustFloat { channels: Some(4) }
    }
}

// ============ 控件配置 ============

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonConfig {
    pub id: String,
    pub label: String,
    pub press_value: f32,
    pub release_value: f32,
    pub binding: WidgetBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioConfig {
    pub id: String,
    pub label: String,
    pub options: Vec<(String, f32)>,
    pub default: usize,
    pub binding: WidgetBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckboxConfig {
    pub id: String,
    pub label: String,
    pub checked_value: f32,
    pub unchecked_value: f32,
    pub default: bool,
    pub binding: WidgetBinding,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelConfig {
    pub id: String,
    pub text: String,
    /// 绑定到接收通道 (可选)
    pub channel: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformConfig {
    pub id: String,
    pub channels: usize,
    /// 每通道最大点数
    pub max_points: usize,
    /// 显示通道列表
    pub visible_channels: Vec<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PieChartConfig {
    pub id: String,
    pub label: String,
    /// 扇区标签
    pub segments: Vec<String>,
    /// 绑定到接收通道
    pub channels: Vec<usize>,
}

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
