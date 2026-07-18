use serde::{Deserialize, Serialize};

// ============ CAN 核心类型 ============

/// CAN 帧方向
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CanDirection {
    Rx,
    Tx,
}

/// CAN 帧 — 标准化 CAN 数据模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFrame {
    pub timestamp: u64,          // 微秒
    pub id: u32,                 // CAN ID (11 位标准 / 29 位扩展)
    pub extended: bool,          // true = 扩展帧
    pub rtr: bool,               // 远程帧
    pub dlc: u8,                 // 数据长度 (0-8)
    pub data: Vec<u8>,           // 数据 (最多 8 字节)
    pub direction: CanDirection, // 收/发方向
}

/// CAN 波特率预设
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CanBitrate {
    Bps100k,
    Bps125k,
    Bps250k,
    Bps500k,
    Bps1m,
}

impl CanBitrate {
    /// 返回波特率数值 (bps)
    pub fn bps(&self) -> u32 {
        match self {
            CanBitrate::Bps100k => 100_000,
            CanBitrate::Bps125k => 125_000,
            CanBitrate::Bps250k => 250_000,
            CanBitrate::Bps500k => 500_000,
            CanBitrate::Bps1m => 1_000_000,
        }
    }

    /// slcan 波特率命令字符 (Lawicel 协议)
    pub fn slcan_cmd(&self) -> &'static str {
        match self {
            CanBitrate::Bps100k => "S3",
            CanBitrate::Bps125k => "S4",
            CanBitrate::Bps250k => "S5",
            CanBitrate::Bps500k => "S6",
            CanBitrate::Bps1m => "S8",
        }
    }
}

/// CAN 过滤器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFilter {
    pub id: u32,
    pub mask: u32,
    pub extended: bool,
}

/// CAN 批次 — 通过 Channel 推送到前端
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFrameBatch {
    pub frames: Vec<CanFrame>,
}

/// candleLight USB 设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandleDeviceInfo {
    pub bus: u8,
    pub address: u8,
    pub vid: u16,
    pub pid: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
}

use std::collections::VecDeque;

/// CAN 帧环形缓冲区 — 用于前端订阅推送
///
/// 当达到 max_size 时, 新帧会挤掉最旧的帧 (FIFO 淘汰)。
pub struct CanBuffer {
    frames: VecDeque<CanFrame>,
    max_size: usize,
}

impl CanBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            frames: VecDeque::with_capacity(max_size.min(8192)),
            max_size,
        }
    }

    /// 推入一帧 (超出容量时丢弃最旧帧)
    pub fn push(&mut self, frame: CanFrame) {
        if self.frames.len() >= self.max_size {
            self.frames.pop_front();
        }
        self.frames.push_back(frame);
    }

    /// 获取最近 count 帧 (按时间顺序返回, 旧的在前)
    pub fn get_recent(&self, count: usize) -> Vec<CanFrame> {
        let n = count.min(self.frames.len());
        self.frames.iter().rev().take(n).rev().cloned().collect()
    }

    /// 清空缓冲区
    pub fn clear(&mut self) {
        self.frames.clear();
    }

    /// 当前缓冲区中的帧数
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// 缓冲区是否为空
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(id: u32, data: Vec<u8>) -> CanFrame {
        CanFrame {
            timestamp: 0,
            id,
            extended: false,
            rtr: false,
            dlc: data.len() as u8,
            data,
            direction: CanDirection::Rx,
        }
    }

    #[test]
    fn test_can_buffer_new_empty() {
        let buf = CanBuffer::new(10);
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_can_buffer_push_and_len() {
        let mut buf = CanBuffer::new(10);
        buf.push(make_frame(0x100, vec![0x01]));
        assert_eq!(buf.len(), 1);
        assert!(!buf.is_empty());
        buf.push(make_frame(0x200, vec![0x02]));
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn test_can_buffer_get_recent_basic() {
        let mut buf = CanBuffer::new(10);
        buf.push(make_frame(0x100, vec![0x01]));
        buf.push(make_frame(0x200, vec![0x02]));
        buf.push(make_frame(0x300, vec![0x03]));
        let recent = buf.get_recent(2);
        assert_eq!(recent.len(), 2);
        // 返回顺序: 旧 → 新
        assert_eq!(recent[0].id, 0x200);
        assert_eq!(recent[1].id, 0x300);
    }

    #[test]
    fn test_can_buffer_get_recent_returns_in_time_order() {
        let mut buf = CanBuffer::new(10);
        for i in 0..5u32 {
            buf.push(make_frame(0x100 + i, vec![i as u8]));
        }
        let recent = buf.get_recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].id, 0x102); // 最旧
        assert_eq!(recent[1].id, 0x103);
        assert_eq!(recent[2].id, 0x104); // 最新
    }

    #[test]
    fn test_can_buffer_get_recent_count_greater_than_len() {
        let mut buf = CanBuffer::new(10);
        buf.push(make_frame(0x100, vec![0x01]));
        buf.push(make_frame(0x200, vec![0x02]));
        // count > len 时应返回全部
        let recent = buf.get_recent(100);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].id, 0x100);
        assert_eq!(recent[1].id, 0x200);
    }

    #[test]
    fn test_can_buffer_get_recent_zero() {
        let mut buf = CanBuffer::new(10);
        buf.push(make_frame(0x100, vec![0x01]));
        let recent = buf.get_recent(0);
        assert_eq!(recent.len(), 0);
    }

    #[test]
    fn test_can_buffer_get_recent_empty_buffer() {
        let buf = CanBuffer::new(10);
        let recent = buf.get_recent(5);
        assert_eq!(recent.len(), 0);
    }

    #[test]
    fn test_can_buffer_clear() {
        let mut buf = CanBuffer::new(10);
        buf.push(make_frame(0x100, vec![0x01]));
        buf.push(make_frame(0x200, vec![0x02]));
        assert_eq!(buf.len(), 2);
        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_can_buffer_clear_idempotent() {
        let mut buf = CanBuffer::new(10);
        buf.clear();
        buf.clear();
        assert!(buf.is_empty());
    }

    #[test]
    fn test_can_buffer_overflow_drops_oldest() {
        let mut buf = CanBuffer::new(3);
        buf.push(make_frame(0x100, vec![0x01]));
        buf.push(make_frame(0x200, vec![0x02]));
        buf.push(make_frame(0x300, vec![0x03]));
        // 第 4 帧应丢弃最旧的 0x100
        buf.push(make_frame(0x400, vec![0x04]));
        assert_eq!(buf.len(), 3);
        let all = buf.get_recent(3);
        assert_eq!(all[0].id, 0x200);
        assert_eq!(all[1].id, 0x300);
        assert_eq!(all[2].id, 0x400);
    }

    #[test]
    fn test_can_buffer_overflow_preserves_recent() {
        // 大量推入后, get_recent 应只返回最近 N 帧
        let mut buf = CanBuffer::new(5);
        for i in 0..10u32 {
            buf.push(make_frame(0x100 + i, vec![i as u8]));
        }
        assert_eq!(buf.len(), 5);
        let recent = buf.get_recent(3);
        assert_eq!(recent.len(), 3);
        // 最近 3 帧为 0x107, 0x108, 0x109
        assert_eq!(recent[0].id, 0x107);
        assert_eq!(recent[1].id, 0x108);
        assert_eq!(recent[2].id, 0x109);
    }

    #[test]
    fn test_can_buffer_max_size_one() {
        // 边界: max_size=1
        let mut buf = CanBuffer::new(1);
        buf.push(make_frame(0x100, vec![0x01]));
        assert_eq!(buf.len(), 1);
        buf.push(make_frame(0x200, vec![0x02]));
        assert_eq!(buf.len(), 1);
        let recent = buf.get_recent(1);
        assert_eq!(recent[0].id, 0x200);
    }

    #[test]
    fn test_can_buffer_preserves_frame_fields() {
        // 验证 push/get_recent 完整保留 CanFrame 字段
        let mut buf = CanBuffer::new(10);
        let original = CanFrame {
            timestamp: 12345,
            id: 0x7FF,
            extended: true,
            rtr: true,
            dlc: 8,
            data: vec![0xDE, 0xAD, 0xBE, 0xEF, 0x12, 0x34, 0x56, 0x78],
            direction: CanDirection::Tx,
        };
        buf.push(original.clone());
        let recent = buf.get_recent(1);
        assert_eq!(recent.len(), 1);
        let f = &recent[0];
        assert_eq!(f.timestamp, original.timestamp);
        assert_eq!(f.id, original.id);
        assert_eq!(f.extended, original.extended);
        assert_eq!(f.rtr, original.rtr);
        assert_eq!(f.dlc, original.dlc);
        assert_eq!(f.data, original.data);
        assert_eq!(f.direction, original.direction);
    }
}
