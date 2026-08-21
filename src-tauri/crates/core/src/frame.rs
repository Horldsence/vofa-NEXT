//! # 数据帧与基础传输类型
//!
//! 跨域共享的"帧"概念,以及连接状态、端口元信息、传输统计。
//!
//! ## 关键类型
//!
//! - [`DataFrame`]: 协议引擎输出的标准格式 — 时间戳 + 多通道 `f32`
//! - [`RawData`]: 未解析的原始字节块 — 用于"原始数据"视图
//! - [`ConnectionState`][]: 离散连接状态机 (Disconnected/Connecting/Connected/Error)
//! - [`PortInfo`][]: 跨平台端口元信息 (含 `vid/pid/serial` 等)
//! - [`TransportStats`][]: 累计传输统计 (rx/tx 字节/帧)
//!
//! ## 时间戳约定
//!
//! `timestamp` 字段一律为**微秒 (µs)**,单调时钟 (`SystemTime::now()` since UNIX_EPOCH)。

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
    /// 自动取当前时间戳构造。
    ///
    /// 中低频路径使用。**批处理**应改用 [`Self::with_timestamp`],
    /// 整批共享同一时间戳,避免每帧一次系统调用。
    pub fn new(channels: Vec<f32>) -> Self {
        Self {
            timestamp: now_us(),
            channels,
        }
    }

    /// 指定时间戳构造 — 高码率协议引擎在每次 feed 只读一次时钟,
    /// 批内所有帧共享同一时间戳 (批间隔 ≤500µs, 远小于显示精度),
    /// 避免每帧一次 `SystemTime::now()` 系统调用
    pub const fn with_timestamp(timestamp: u64, channels: Vec<f32>) -> Self {
        Self {
            timestamp,
            channels,
        }
    }

    /// 通道数
    pub const fn len(&self) -> usize {
        self.channels.len()
    }

    /// 是否为空帧 (无通道数据)
    pub const fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }
}

/// 原始数据块 — 未经协议解析的字节流
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawData {
    pub timestamp: u64,
    pub data: Vec<u8>,
}

impl RawData {
    /// 构造给定时间戳的原始字节块
    pub const fn new(timestamp: u64, data: Vec<u8>) -> Self {
        Self { timestamp, data }
    }
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
    pub description: Option<String>,
}

/// 传输统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransportStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_frames: u64,
    pub tx_frames: u64,
    /// 最近 100ms 统计窗口内 broadcast Lagged 丢弃的消息数
    #[serde(default)]
    pub rx_dropped: u64,
}

#[allow(clippy::cast_possible_truncation)]
/// 单调时钟当前微秒数 — UNIX_EPOCH 起算。
///
/// 时钟倒退时返回 0(单调时钟通常不应倒退,降级安全)。
pub fn now_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}
