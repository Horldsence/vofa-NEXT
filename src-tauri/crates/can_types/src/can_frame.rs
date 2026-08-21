//! CAN 帧、方向、波特率、过滤、批次与 candleLight 设备信息

use serde::{Deserialize, Serialize};

// ============ CAN 帧基础类型 ============

/// CAN 帧方向
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum CanDirection {
    #[default]
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

impl CanFrame {
    /// 构造给定时间戳、ID、方向与数据的 CAN 帧
    ///
    /// `data` 超过 8 字节时自动截断,以匹配 CAN DLC 上限。
    #[allow(clippy::cast_possible_truncation)]
    pub fn new(timestamp: u64, id: u32, data: Vec<u8>, direction: CanDirection) -> Self {
        let dlc = u8::try_from(data.len().min(8)).expect("dlc 已限制为 8");
        let data = data.into_iter().take(dlc as usize).collect();
        Self {
            timestamp,
            id,
            extended: false,
            rtr: false,
            dlc,
            data,
            direction,
        }
    }

    /// 数据字节数 (基于 DLC)
    pub const fn data_len(&self) -> usize {
        self.dlc as usize
    }
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

// ============ CAN 过滤与批次 ============

/// CAN 过滤器配置 — 通过 ID 位掩码控制哪些帧通过
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFilter {
    /// 是否启用
    pub enabled: bool,
    /// 标准帧 ID 掩码 (只保留低 11 位中掩码为 1 的位)
    pub id_mask_std: u16,
    /// 扩展帧 ID 掩码 (只保留低 29 位中掩码为 1 的位)
    pub id_mask_ext: u32,
}

impl CanFilter {
    /// 检查帧是否匹配过滤条件
    ///
    /// 未启用时恒匹配;启用后按标准/扩展帧分别应用 ID 掩码,
    /// `(frame.id & mask) != 0` 即视为匹配。
    pub const fn matches(&self, frame: &CanFrame) -> bool {
        if !self.enabled {
            return true;
        }
        if frame.extended {
            (frame.id & self.id_mask_ext) != 0
        } else {
            (frame.id & self.id_mask_std as u32) != 0
        }
    }
}

/// CAN 帧过滤器匹配器 — 方向 + 白/黑名单
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
        if self.rx_only && frame.direction != CanDirection::Rx {
            return false;
        }
        if self.tx_only && frame.direction != CanDirection::Tx {
            return false;
        }
        if !self.id_whitelist.is_empty() && !self.id_whitelist.contains(&frame.id) {
            return false;
        }
        if self.id_blacklist.contains(&frame.id) {
            return false;
        }
        true
    }
}

/// CAN 帧批次 (用于批量传输) — 单调递增 `seq`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFrameBatch {
    pub seq: u64,
    pub frames: Vec<CanFrame>,
}

impl CanFrameBatch {
    /// 构造空批次
    pub const fn new(seq: u64) -> Self {
        Self {
            seq,
            frames: Vec::new(),
        }
    }

    /// 帧数
    pub const fn len(&self) -> usize {
        self.frames.len()
    }

    /// 是否空批
    pub const fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
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
