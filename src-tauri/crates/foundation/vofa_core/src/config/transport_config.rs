//! 传输层配置
//!
//! 涵盖所有 transport backend 的可调参数:
//! - [`TransportConfig`] tagged enum,前端按 `kind` 选择 backend
//! - [`SerialConfig`] / [`UdpConfig`] / [`TcpClientConfig`] / [`TcpServerConfig`]
//! - [`TestDataConfig`] (内置测试数据源) / [`SlcanConfig`] / [`CandleConfig`] (CAN 桥)
//! - [`TestSignal`] 波形枚举

use serde::{Deserialize, Serialize};

use can_types::CanBitrate;

use crate::serial_params::{FlowControl, Parity, StopBits};

/// 传输层后端 — tagged enum, 序列化 `{ kind, params }`
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

/// 串口传输参数
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

/// UDP 传输参数
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

/// TCP 客户端参数
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

/// TCP 服务端参数
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

/// 内置测试数据源参数
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

/// 测试信号波形类型
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
