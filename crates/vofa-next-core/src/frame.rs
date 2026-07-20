use serde::{Deserialize, Serialize};

/// 解析后的数据帧 — 协议引擎输出的标准格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFrame {
    /// 时间戳 (微秒, monotonic)
    pub timestamp: u64,
    /// 多通道浮点数据
    pub channels: Vec<f32>,
}

impl DataFrame {
    pub fn new(channels: Vec<f32>) -> Self {
        Self {
            timestamp: now_us(),
            channels,
        }
    }
}

/// 原始数据块 — 未经协议解析的字节流
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawData {
    pub timestamp: u64,
    pub data: Vec<u8>,
}

/// 连接状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// 串口端口信息 (跨平台)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortInfo {
    pub name: String,
    pub port_type: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

/// 传输统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransportStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_frames: u64,
    pub tx_frames: u64,
}

#[allow(clippy::cast_possible_truncation)]
fn now_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}
