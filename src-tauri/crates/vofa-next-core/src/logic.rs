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

use std::collections::VecDeque;

/// 逻辑采样环形缓冲区 — 用于前端订阅查询
pub struct LogicBuffer {
    samples: VecDeque<LogicSample>,
    max_size: usize,
}

impl LogicBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_size.min(16384)),
            max_size,
        }
    }

    pub fn push(&mut self, sample: LogicSample) {
        if self.samples.len() >= self.max_size {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    /// 获取最近 n 个采样 (返回顺序: 旧→新)
    pub fn get_recent(&self, count: usize) -> Vec<LogicSample> {
        let n = count.min(self.samples.len());
        self.samples
            .iter()
            .rev()
            .take(n)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size;
        while self.samples.len() > max_size {
            self.samples.pop_front();
        }
    }
}

/// 解码事件环形缓冲区
pub struct DecodedBuffer {
    events: VecDeque<DecodedEvent>,
    max_size: usize,
}

impl DecodedBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(max_size.min(8192)),
            max_size,
        }
    }

    pub fn push(&mut self, event: DecodedEvent) {
        if self.events.len() >= self.max_size {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    pub fn get_recent(&self, count: usize) -> Vec<DecodedEvent> {
        let n = count.min(self.events.len());
        self.events
            .iter()
            .rev()
            .take(n)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size.max(1);
        while self.events.len() > self.max_size {
            self.events.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// 逻辑采样批次 — 通过 Channel 推送到前端
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogicSampleBatch {
    pub samples: Vec<LogicSample>,
}

/// 解码事件批次 — 通过 Channel 推送到前端
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DecodedEventBatch {
    pub events: Vec<DecodedEvent>,
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;

    fn make_sample(ts: u64, channels: u32) -> LogicSample {
        LogicSample {
            timestamp: ts,
            channels,
            channel_count: 8,
        }
    }

    fn make_uart_event(ts: u64, byte: u8) -> DecodedEvent {
        DecodedEvent::Uart {
            timestamp: ts,
            byte,
            parity_ok: true,
        }
    }

    // ===== LogicBuffer tests =====

    #[test]
    fn test_logic_buffer_new_empty() {
        let buf = LogicBuffer::new(10);
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_logic_buffer_push_and_len() {
        let mut buf = LogicBuffer::new(10);
        buf.push(make_sample(0, 0xFF));
        buf.push(make_sample(1, 0xAA));
        assert_eq!(buf.len(), 2);
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_logic_buffer_get_recent_basic() {
        let mut buf = LogicBuffer::new(10);
        buf.push(make_sample(0, 0x01));
        buf.push(make_sample(1, 0x02));
        buf.push(make_sample(2, 0x03));
        let recent = buf.get_recent(2);
        assert_eq!(recent.len(), 2);
        // 顺序: 旧 → 新
        assert_eq!(recent[0].timestamp, 1);
        assert_eq!(recent[1].timestamp, 2);
    }

    #[test]
    fn test_logic_buffer_get_recent_returns_in_time_order() {
        let mut buf = LogicBuffer::new(10);
        for i in 0..5u64 {
            buf.push(make_sample(i, i as u32));
        }
        let recent = buf.get_recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].timestamp, 2); // 最旧
        assert_eq!(recent[1].timestamp, 3);
        assert_eq!(recent[2].timestamp, 4); // 最新
    }

    #[test]
    fn test_logic_buffer_get_recent_count_greater_than_len() {
        let mut buf = LogicBuffer::new(10);
        buf.push(make_sample(0, 0x01));
        buf.push(make_sample(1, 0x02));
        let recent = buf.get_recent(100);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn test_logic_buffer_get_recent_zero() {
        let mut buf = LogicBuffer::new(10);
        buf.push(make_sample(0, 0x01));
        let recent = buf.get_recent(0);
        assert_eq!(recent.len(), 0);
    }

    #[test]
    fn test_logic_buffer_get_recent_empty_buffer() {
        let buf = LogicBuffer::new(10);
        let recent = buf.get_recent(5);
        assert_eq!(recent.len(), 0);
    }

    #[test]
    fn test_logic_buffer_clear() {
        let mut buf = LogicBuffer::new(10);
        buf.push(make_sample(0, 0x01));
        buf.push(make_sample(1, 0x02));
        assert_eq!(buf.len(), 2);
        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_logic_buffer_overflow_drops_oldest() {
        let mut buf = LogicBuffer::new(3);
        buf.push(make_sample(0, 0x01));
        buf.push(make_sample(1, 0x02));
        buf.push(make_sample(2, 0x03));
        // 第 4 个应丢弃 ts=0
        buf.push(make_sample(3, 0x04));
        assert_eq!(buf.len(), 3);
        let all = buf.get_recent(3);
        assert_eq!(all[0].timestamp, 1);
        assert_eq!(all[1].timestamp, 2);
        assert_eq!(all[2].timestamp, 3);
    }

    #[test]
    fn test_logic_buffer_overflow_preserves_recent() {
        let mut buf = LogicBuffer::new(5);
        for i in 0..10u64 {
            buf.push(make_sample(i, i as u32));
        }
        assert_eq!(buf.len(), 5);
        let recent = buf.get_recent(3);
        assert_eq!(recent.len(), 3);
        // 最近 3 个采样 ts = 7, 8, 9
        assert_eq!(recent[0].timestamp, 7);
        assert_eq!(recent[1].timestamp, 8);
        assert_eq!(recent[2].timestamp, 9);
    }

    #[test]
    fn test_logic_buffer_set_max_size_trims() {
        let mut buf = LogicBuffer::new(10);
        for i in 0..8u64 {
            buf.push(make_sample(i, i as u32));
        }
        assert_eq!(buf.len(), 8);
        // 缩小到 4
        buf.set_max_size(4);
        assert_eq!(buf.len(), 4);
        // 应保留最近 4 个 (ts 4..8)
        let all = buf.get_recent(4);
        assert_eq!(all[0].timestamp, 4);
        assert_eq!(all[3].timestamp, 7);
    }

    #[test]
    fn test_logic_buffer_set_max_size_grows() {
        let mut buf = LogicBuffer::new(3);
        for i in 0..3u64 {
            buf.push(make_sample(i, i as u32));
        }
        // 扩大到 10, 应不丢失现有数据
        buf.set_max_size(10);
        assert_eq!(buf.len(), 3);
        // 推入 7 个新采样 (3..10), 总数 = 3 + 7 = 10, 刚好填满
        for i in 3..10u64 {
            buf.push(make_sample(i, i as u32));
        }
        assert_eq!(buf.len(), 10);
        // 验证数据完整 (旧 → 新)
        let all = buf.get_recent(10);
        assert_eq!(all[0].timestamp, 0);
        assert_eq!(all[9].timestamp, 9);
    }

    #[test]
    fn test_logic_buffer_max_size_one() {
        let mut buf = LogicBuffer::new(1);
        buf.push(make_sample(0, 0x01));
        assert_eq!(buf.len(), 1);
        buf.push(make_sample(1, 0x02));
        assert_eq!(buf.len(), 1);
        let recent = buf.get_recent(1);
        assert_eq!(recent[0].timestamp, 1);
    }

    #[test]
    fn test_logic_buffer_preserves_sample_fields() {
        let mut buf = LogicBuffer::new(10);
        let original = LogicSample {
            timestamp: 99999,
            channels: 0b10101010,
            channel_count: 4,
        };
        buf.push(original.clone());
        let recent = buf.get_recent(1);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].timestamp, original.timestamp);
        assert_eq!(recent[0].channels, original.channels);
        assert_eq!(recent[0].channel_count, original.channel_count);
    }

    // ===== DecodedBuffer tests =====

    #[test]
    fn test_decoded_buffer_new_empty() {
        let buf = DecodedBuffer::new(10);
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_decoded_buffer_push_and_get_recent() {
        let mut buf = DecodedBuffer::new(10);
        buf.push(make_uart_event(0, 0x41));
        buf.push(make_uart_event(1, 0x42));
        buf.push(make_uart_event(2, 0x43));
        assert_eq!(buf.len(), 3);
        let recent = buf.get_recent(2);
        assert_eq!(recent.len(), 2);
        // 顺序: 旧 → 新
        match &recent[0] {
            DecodedEvent::Uart { byte, .. } => assert_eq!(*byte, 0x42),
            _ => panic!("期望 UART 事件"),
        }
        match &recent[1] {
            DecodedEvent::Uart { byte, .. } => assert_eq!(*byte, 0x43),
            _ => panic!("期望 UART 事件"),
        }
    }

    #[test]
    fn test_decoded_buffer_clear() {
        let mut buf = DecodedBuffer::new(10);
        buf.push(make_uart_event(0, 0x41));
        buf.push(make_uart_event(1, 0x42));
        assert_eq!(buf.len(), 2);
        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_decoded_buffer_overflow_drops_oldest() {
        let mut buf = DecodedBuffer::new(3);
        buf.push(make_uart_event(0, 0x41));
        buf.push(make_uart_event(1, 0x42));
        buf.push(make_uart_event(2, 0x43));
        // 第 4 个应丢弃 0x41
        buf.push(make_uart_event(3, 0x44));
        assert_eq!(buf.len(), 3);
        let all = buf.get_recent(3);
        match &all[0] {
            DecodedEvent::Uart { byte, .. } => assert_eq!(*byte, 0x42),
            _ => panic!("期望 UART 事件"),
        }
        match &all[2] {
            DecodedEvent::Uart { byte, .. } => assert_eq!(*byte, 0x44),
            _ => panic!("期望 UART 事件"),
        }
    }

    #[test]
    fn test_decoded_buffer_get_recent_count_greater_than_len() {
        let mut buf = DecodedBuffer::new(10);
        buf.push(make_uart_event(0, 0x41));
        buf.push(make_uart_event(1, 0x42));
        let recent = buf.get_recent(100);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn test_decoded_buffer_get_recent_zero() {
        let mut buf = DecodedBuffer::new(10);
        buf.push(make_uart_event(0, 0x41));
        let recent = buf.get_recent(0);
        assert_eq!(recent.len(), 0);
    }

    #[test]
    fn test_decoded_buffer_max_size_one() {
        let mut buf = DecodedBuffer::new(1);
        buf.push(make_uart_event(0, 0x41));
        buf.push(make_uart_event(1, 0x42));
        assert_eq!(buf.len(), 1);
        let recent = buf.get_recent(1);
        match &recent[0] {
            DecodedEvent::Uart { byte, .. } => assert_eq!(*byte, 0x42),
            _ => panic!("期望 UART 事件"),
        }
    }

    #[test]
    fn test_decoded_buffer_preserves_i2c_event() {
        let mut buf = DecodedBuffer::new(10);
        let original = DecodedEvent::I2c {
            timestamp: 12345,
            event: I2cEvent::Address {
                addr: 0x50,
                read: true,
                ack: false,
            },
        };
        buf.push(original);
        let recent = buf.get_recent(1);
        assert_eq!(recent.len(), 1);
        match &recent[0] {
            DecodedEvent::I2c { timestamp, event } => {
                assert_eq!(*timestamp, 12345);
                match event {
                    I2cEvent::Address { addr, read, ack } => {
                        assert_eq!(*addr, 0x50);
                        assert!(*read);
                        assert!(!*ack);
                    }
                    _ => panic!("期望 Address 事件"),
                }
            }
            _ => panic!("期望 I2C 事件"),
        }
    }

    #[test]
    fn test_decoded_buffer_preserves_spi_event() {
        let mut buf = DecodedBuffer::new(10);
        let original = DecodedEvent::Spi {
            timestamp: 999,
            mosi: 0xA5,
            miso: 0x3C,
        };
        buf.push(original);
        let recent = buf.get_recent(1);
        match &recent[0] {
            DecodedEvent::Spi {
                timestamp,
                mosi,
                miso,
            } => {
                assert_eq!(*timestamp, 999);
                assert_eq!(*mosi, 0xA5);
                assert_eq!(*miso, 0x3C);
            }
            _ => panic!("期望 SPI 事件"),
        }
    }
}
