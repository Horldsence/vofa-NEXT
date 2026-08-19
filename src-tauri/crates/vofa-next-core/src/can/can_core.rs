//! CAN 核心类型 — 帧、缓冲区、设备信息

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

// ============ CAN 帧核心类型 ============

/// CAN 帧方向
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CanDirection {
    Rx,
    Tx,
}

/// CAN 帧 — 标准化 CAN 数据模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFrame {
    pub timestamp: u64,
    pub id: u32,
    pub extended: bool,
    pub rtr: bool,
    pub dlc: u8,
    pub data: Vec<u8>,
    pub direction: CanDirection,
}

/// CAN 波特率
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CanBitrate {
    Bps100k,
    Bps125k,
    Bps250k,
    Bps500k,
    Bps1m,
}

impl CanBitrate {
    /// 返回波特率数值 (bps)
    pub const fn bps(&self) -> u32 {
        match self {
            Self::Bps100k => 100_000,
            Self::Bps125k => 125_000,
            Self::Bps250k => 250_000,
            Self::Bps500k => 500_000,
            Self::Bps1m => 1_000_000,
        }
    }

    /// slcan 波特率命令字符 (Lawicel 协议)
    pub const fn slcan_cmd(&self) -> &'static str {
        match self {
            Self::Bps100k => "S3",
            Self::Bps125k => "S4",
            Self::Bps250k => "S5",
            Self::Bps500k => "S6",
            Self::Bps1m => "S8",
        }
    }
}

/// CAN 过滤器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFilter {
    /// 是否启用
    pub enabled: bool,
    /// 标准帧 ID 掩码 (只保留低 11 位中掩码为 1 的位)
    pub id_mask_std: u16,
    /// 扩展帧 ID 掩码 (只保留低 29 位中掩码为 1 的位)
    pub id_mask_ext: u32,
}

/// CAN 帧过滤器 — 决定哪些帧应该被保留
impl CanFilter {
    /// 检查帧是否匹配过滤条件
    pub const fn matches(&self, frame: &CanFrame) -> bool {
        if !self.enabled {
            return true;
        }
        if frame.extended {
            (frame.id & self.id_mask_ext) == (frame.id & self.id_mask_ext)
        } else {
            ((frame.id as u16) & self.id_mask_std) == ((frame.id as u16) & self.id_mask_std)
        }
    }
}

/// CAN 帧过滤器匹配器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFrameFilter {
    /// 是否只接收 Rx 帧
    pub rx_only: bool,
    /// 是否只接收 Tx 帧
    pub tx_only: bool,
    /// ID 白名单 (空表示不限制)
    pub id_whitelist: Vec<u32>,
    /// ID 黑名单
    pub id_blacklist: Vec<u32>,
}

impl CanFrameFilter {
    /// 检查帧是否匹配过滤条件
    pub fn matches(&self, frame: &CanFrame) -> bool {
        // 方向过滤
        if self.rx_only && frame.direction != CanDirection::Rx {
            return false;
        }
        if self.tx_only && frame.direction != CanDirection::Tx {
            return false;
        }
        // 白名单过滤
        if !self.id_whitelist.is_empty() && !self.id_whitelist.contains(&frame.id) {
            return false;
        }
        // 黑名单过滤
        if self.id_blacklist.contains(&frame.id) {
            return false;
        }
        true
    }
}

/// CAN 帧批次 (用于批量传输)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFrameBatch {
    pub seq: u64,
    pub frames: Vec<CanFrame>,
}

/// candleLight 设备信息
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

// ============ CAN 缓冲区 ============

/// CAN 帧环形缓冲区
#[derive(Debug)]
pub struct CanBuffer {
    /// 帧存储 (环形缓冲区)
    frames: VecDeque<CanFrame>,
    /// 最大帧数
    max_size: usize,
    /// 版本号 (单调递增, push 时变化)
    version: u64,
}

impl CanBuffer {
    /// 创建指定容量的 CAN 缓冲区
    pub fn new(max_size: usize) -> Self {
        Self {
            frames: VecDeque::with_capacity(max_size.min(8192)),
            max_size,
            version: 0,
        }
    }

    /// 推入一帧 (超出容量时丢弃最旧帧)
    pub fn push(&mut self, frame: CanFrame) {
        if self.frames.len() >= self.max_size {
            self.frames.pop_front();
        }
        self.frames.push_back(frame);
        self.version = self.version.wrapping_add(1);
    }

    /// 当前版本号 (单调递增, push 时变化)
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 获取最近 n 帧 (按时间顺序返回, 旧的在前)
    pub fn get_recent(&self, count: usize) -> Vec<CanFrame> {
        let n = count.min(self.frames.len());
        self.frames.iter().rev().take(n).rev().cloned().collect()
    }

    /// 增量游标读取 — 统一分片流用
    ///
    /// cursor 为绝对序号 (version = 累计 push 数)。可读区间 = [max(cursor, version-len), version);
    /// 游标若已被驱逐越过则顺移并计入 dropped。
    /// 返回 (items, new_cursor, dropped)。
    pub fn drain_from(&self, cursor: u64, max: usize) -> (Vec<CanFrame>, u64, u64) {
        let version = self.version;
        let oldest = version - self.frames.len() as u64;
        let start = cursor.max(oldest);
        let dropped = start.saturating_sub(cursor);
        let n = usize::try_from(version - start).unwrap_or(max).min(max);
        let skip = usize::try_from(start - oldest).unwrap_or(0);
        let items = self.frames.iter().skip(skip).take(n).cloned().collect();
        (items, start + n as u64, dropped)
    }

    /// 清空缓冲区
    pub fn clear(&mut self) {
        self.frames.clear();
    }

    /// 设置最大容量 (保留最近帧)
    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size.max(1);
        while self.frames.len() > self.max_size {
            self.frames.pop_front();
        }
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
