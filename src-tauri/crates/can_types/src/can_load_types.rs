//! CAN 负载统计相关纯数据类型 — 快照、历史采样、按 ID 分布
//!
//! 实现/计算逻辑在 [`crate::can_load_stats`]。

use serde::{Deserialize, Serialize};

/// 单个 ID 的负载统计快照
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CanIdLoadStats {
    pub id: u32,
    pub extended: bool,
    pub frame_count: u64,
    /// 总位数(含位填充估算)
    pub total_bits: u64,
    /// 总字节数(DLC 累加)
    pub total_bytes: u64,
}

/// CAN 负载统计快照 — 由滑动窗口计算得到
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanLoadSnapshot {
    /// 窗口大小(微秒)
    pub window_us: u64,
    /// 窗口内总帧数
    pub frame_count: u64,
    /// 窗口内总位数(含位填充估算)
    pub total_bits: u64,
    /// 窗口内总字节数
    pub total_bytes: u64,
    /// 当前负载率(0.0 - 1.0+,可超过 1.0 表示过载)
    pub load_ratio: f64,
    /// 时间序列采样(最近的负载率历史,用于绘制折线图)
    pub history: Vec<CanLoadHistoryPoint>,
    /// 按 ID 的负载分布(按 `total_bits` 降序)
    pub per_id: Vec<CanIdLoadStats>,
    /// 按 ID 的负载率历史(用于时序图叠加显示)
    pub per_id_history: Vec<CanIdLoadHistory>,
}

/// 单个 ID 的负载率历史(用于时序图叠加)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanIdLoadHistory {
    pub id: u32,
    pub extended: bool,
    pub history: Vec<CanLoadHistoryPoint>,
}

/// 负载历史采样点
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CanLoadHistoryPoint {
    /// 时间戳(微秒)
    pub timestamp: u64,
    /// 负载率(0.0 - 1.0+)
    pub load_ratio: f64,
    /// 帧率(帧/秒)
    pub fps: f64,
}
