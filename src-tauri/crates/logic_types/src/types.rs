//! 逻辑分析仪核心类型 — 采样、事件、过滤条件、解码器配置

use serde::{Deserialize, Serialize};

use vofa_core::{Parity, StopBits};

// ============ 采样与事件 ============

/// 逻辑分析仪采样 — 多通道数字电平快照
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogicSample {
    /// 时间戳(微秒)
    pub timestamp: u64,
    /// 位图,`bit i` = 通道 i 的电平 (0/1)
    pub channels: u32,
    /// 实际通道数
    pub channel_count: u8,
}

impl LogicSample {
    /// 构造单采样
    pub const fn new(timestamp: u64, channels: u32, channel_count: u8) -> Self {
        Self {
            timestamp,
            channels,
            channel_count,
        }
    }

    /// 读单通道电平
    pub const fn channel(&self, idx: u8) -> bool {
        (self.channels >> idx) & 1 == 1
    }
}

/// 逻辑采样批次(无序集合)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LogicBatch {
    pub samples: Vec<LogicSample>,
}

impl LogicBatch {
    /// 构造空批次
    pub fn new() -> Self {
        Self::default()
    }

    /// 推入一个采样
    pub fn push(&mut self, s: LogicSample) {
        self.samples.push(s);
    }

    /// 采样数
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// 是否空
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// I2C 事件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum I2cEvent {
    Start,
    Stop,
    Address { addr: u8, read: bool, ack: bool },
    Data { byte: u8, ack: bool },
}

/// 协议解码结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

impl DecodedEvent {
    /// 事件协议名(小写),用于 `DecodedEventFilter.kind` 匹配
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Uart { .. } => "uart",
            Self::I2c { .. } => "i2c",
            Self::Spi { .. } => "spi",
        }
    }

    /// 事件时间戳
    pub const fn timestamp(&self) -> u64 {
        match self {
            Self::Uart { timestamp, .. }
            | Self::I2c { timestamp, .. }
            | Self::Spi { timestamp, .. } => *timestamp,
        }
    }
}

// ============ 过滤条件 ============

/// 逻辑采样过滤条件 — 用于后端订阅过滤
///
/// 所有字段为 None 时匹配全部采样;设置掩码后按
/// `(channels & mask) == (value & mask)` 匹配(value 缺省为 0)。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct LogicSampleFilter {
    /// 通道位图掩码 — 只关心这些通道
    pub channel_mask: Option<u32>,
    /// 期望通道值(与掩码组合使用)
    pub channel_value: Option<u32>,
}

impl LogicSampleFilter {
    /// 是否全 None(无过滤)
    pub const fn is_empty(&self) -> bool {
        self.channel_mask.is_none() && self.channel_value.is_none()
    }

    /// 判断指定采样是否匹配本过滤条件
    pub const fn matches(&self, sample: &LogicSample) -> bool {
        if let Some(mask) = self.channel_mask {
            let value = match self.channel_value {
                Some(v) => v,
                None => 0,
            };
            if (sample.channels & mask) != (value & mask) {
                return false;
            }
        }
        true
    }
}

/// 解码事件过滤条件 — 用于后端订阅过滤
///
/// 所有字段为 None 时匹配全部事件;`kind` 为协议名("uart"/"i2c"/"spi"),
/// `byte_pattern` 对事件载荷字节(UART byte / I2C addr+data / SPI mosi+miso)
/// 做子串匹配。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DecodedEventFilter {
    /// 协议类型过滤(大小写不敏感): "uart" | "i2c" | "spi"
    pub kind: Option<String>,
    /// 载荷字节子串匹配
    pub byte_pattern: Option<Vec<u8>>,
}

impl DecodedEventFilter {
    /// 是否全 None(无过滤)
    pub const fn is_empty(&self) -> bool {
        self.kind.is_none() && self.byte_pattern.is_none()
    }

    /// 事件协议名(小写)是否与过滤器匹配
    fn kind_matches(event_kind: &str, kind: &str) -> bool {
        kind.eq_ignore_ascii_case(event_kind)
    }

    /// 提取事件载荷字节
    fn payload(event: &DecodedEvent) -> Vec<u8> {
        match event {
            DecodedEvent::Uart { byte, .. } => vec![*byte],
            DecodedEvent::I2c { event, .. } => match event {
                I2cEvent::Address { addr, .. } => vec![*addr],
                I2cEvent::Data { byte, .. } => vec![*byte],
                I2cEvent::Start | I2cEvent::Stop => Vec::new(),
            },
            DecodedEvent::Spi { mosi, miso, .. } => vec![*mosi, *miso],
        }
    }

    /// 子串匹配 (`pattern` 为空时返回 true 表示"全部通过")
    fn byte_pattern_matches(payload: &[u8], pattern: &[u8]) -> bool {
        if pattern.is_empty() {
            return true;
        }
        if payload.len() < pattern.len() {
            return false;
        }
        payload
            .windows(pattern.len())
            .any(|w| w == pattern)
    }

    /// 判断指定事件是否匹配本过滤条件
    pub fn matches(&self, event: &DecodedEvent) -> bool {
        if let Some(kind) = &self.kind {
            if !Self::kind_matches(event.kind(), kind) {
                return false;
            }
        }
        if let Some(pattern) = &self.byte_pattern {
            let payload = Self::payload(event);
            if !Self::byte_pattern_matches(&payload, pattern) {
                return false;
            }
        }
        true
    }
}

// ============ 解码器配置 ============

/// 解码器配置
///
/// `PartialEq`:`schema` 模型中 `DecoderBlockDef::Samples` 内嵌本配置,块列表
/// 比较(`FrameParser::matches_config`)需要逐字段相等判定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
        /// SPI mode 0-3
        mode: u8,
    },
}

impl LogicDecoderConfig {
    /// 协议名(小写)
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Uart { .. } => "uart",
            Self::I2c { .. } => "i2c",
            Self::Spi { .. } => "spi",
        }
    }
}

// ============ 批次类型 ============

/// 逻辑采样批次 — 通过 Channel 推送到前端
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LogicSampleBatch {
    /// 组级单调序号 — 分片并发推送时前端按 `seq` 重组
    #[serde(default)]
    pub seq: u64,
    pub samples: Vec<LogicSample>,
}

/// 解码事件批次 — 通过 Channel 推送到前端
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecodedEventBatch {
    /// 组级单调序号 — 分片并发推送时前端按 `seq` 重组
    #[serde(default)]
    pub seq: u64,
    pub events: Vec<DecodedEvent>,
}
