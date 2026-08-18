//! 逻辑分析仪核心类型 — 采样、事件、过滤条件

use serde::{Deserialize, Serialize};

use crate::config::{Parity, StopBits};

// ============ 逻辑分析仪类型 ============

/// 逻辑分析仪采样 — 多通道数字电平快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogicSample {
    pub timestamp: u64,    // 微秒
    pub channels: u32,     // 位图, bit i = 通道 i 的电平 (0/1)
    pub channel_count: u8, // 实际通道数
}

/// 逻辑样本批次
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogicBatch {
    pub samples: Vec<LogicSample>,
}

/// I2C 事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum I2cEvent {
    Start,
    Stop,
    Address { addr: u8, read: bool, ack: bool },
    Data { byte: u8, ack: bool },
}

/// 协议解码结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecodedEvent {
    Uart {
        timestamp: u64,
        byte: u8,
        parity_ok: bool,
    },
    I2c {
        timestamp: u64,
        event: I2cEvent,
    },
    Spi {
        timestamp: u64,
        mosi: u8,
        miso: u8,
    },
}

/// 逻辑采样过滤条件 — 用于后端订阅过滤
///
/// 所有字段为 None 时匹配全部采样; 设置掩码后按
/// `(channels & mask) == (value & mask)` 匹配 (value 缺省为 0)。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LogicSampleFilter {
    /// 通道位图掩码 — 只关心这些通道
    pub channel_mask: Option<u32>,
    /// 期望通道值 (与掩码组合使用)
    pub channel_value: Option<u32>,
}

impl LogicSampleFilter {
    /// 判断指定采样是否匹配本过滤条件
    pub fn matches(&self, sample: &LogicSample) -> bool {
        if let Some(mask) = self.channel_mask {
            let value = self.channel_value.unwrap_or(0);
            if (sample.channels & mask) != (value & mask) {
                return false;
            }
        }
        true
    }
}

/// 解码事件过滤条件 — 用于后端订阅过滤
///
/// 所有字段为 None 时匹配全部事件; kind 为协议名 ("uart"/"i2c"/"spi"),
/// byte_pattern 对事件载荷字节 (UART byte / I2C addr+data / SPI mosi+miso) 做子串匹配。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecodedEventFilter {
    /// 协议类型过滤 (大小写不敏感): "uart" | "i2c" | "spi"
    pub kind: Option<String>,
    /// 载荷字节子串匹配
    pub byte_pattern: Option<Vec<u8>>,
}

impl DecodedEventFilter {
    /// 判断指定事件是否匹配本过滤条件
    pub fn matches(&self, event: &DecodedEvent) -> bool {
        if let Some(kind) = &self.kind {
            let event_kind = match event {
                DecodedEvent::Uart { .. } => "uart",
                DecodedEvent::I2c { .. } => "i2c",
                DecodedEvent::Spi { .. } => "spi",
            };
            if !kind.eq_ignore_ascii_case(event_kind) {
                return false;
            }
        }
        if let Some(pattern) = &self.byte_pattern {
            if pattern.is_empty() {
                return true;
            }
            // 收集事件载荷字节, 做子串匹配
            let payload: Vec<u8> = match event {
                DecodedEvent::Uart { byte, .. } => vec![*byte],
                DecodedEvent::I2c { event, .. } => match event {
                    I2cEvent::Address { addr, .. } => vec![*addr],
                    I2cEvent::Data { byte, .. } => vec![*byte],
                    I2cEvent::Start | I2cEvent::Stop => Vec::new(),
                },
                DecodedEvent::Spi { mosi, miso, .. } => vec![*mosi, *miso],
            };
            if payload.len() < pattern.len()
                || !payload
                    .windows(pattern.len())
                    .any(|w| w == pattern.as_slice())
            {
                return false;
            }
        }
        true
    }
}

/// 解码器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "params")]
pub enum LogicDecoderConfig {
    Uart {
        baud_rate: u32,
        data_bits: u8,
        parity: Parity,
        stop_bits: StopBits,
        channel: u8,
    },
    I2c {
        sda_channel: u8,
        scl_channel: u8,
    },
    Spi {
        sclk_channel: u8,
        mosi_channel: u8,
        miso_channel: u8,
        cs_channel: u8,
        mode: u8, // 0-3
    },
}

/// 逻辑采样批次 — 通过 Channel 推送到前端
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogicSampleBatch {
    /// 组级单调序号 — 分片并发推送时前端按 seq 重组
    #[serde(default)]
    pub seq: u64,
    pub samples: Vec<LogicSample>,
}

/// 解码事件批次 — 通过 Channel 推送到前端
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DecodedEventBatch {
    /// 组级单调序号 — 分片并发推送时前端按 seq 重组
    #[serde(default)]
    pub seq: u64,
    pub events: Vec<DecodedEvent>,
}
