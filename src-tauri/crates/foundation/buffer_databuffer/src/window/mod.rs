//! `DataBuffer` 窗口切片 — WaveformWindow 查询 (get_window / get_recent) 与 NaN 对齐
//!
//! 派生缓冲区创建较晚时, 窗口早期位置填 NaN (表示 "尚无数据"),
//! 保证 derived[i] 与 channels[ch][i] 严格按时间戳对齐。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

mod downsample;
mod query;
mod snapshot;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DerivedSeriesSelector {
    pub sink_id: String,
    pub source_id: String,
    #[serde(default)]
    pub source_handle: String,
}

/// 决定显示采样时参与计算的序列；输出仍保持完整通道槽形状。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WaveformSeriesSelection {
    #[serde(default)]
    pub channels: Vec<usize>,
    #[serde(default)]
    pub derived: Vec<DerivedSeriesSelector>,
}

/// 窗口内样本的生成策略。IPC 消费方据此区分原始数据与显示包络，避免把降采样
/// 结果误用于导出或测量。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WaveformSampling {
    #[default]
    Raw,
    MinMax,
    Lttb,
}

#[derive(Debug, Clone)]
enum SeriesTarget {
    Channel(usize),
    Derived {
        sink: String,
        source: String,
        handle: String,
    },
}

/// 显示序列的纯数据快照 (值数组与窗口时间戳等长, 已 NaN 对齐)
#[derive(Debug, Clone)]
struct SnapshotSeries {
    target: SeriesTarget,
    values: Vec<f32>,
}

/// 窗口快照 — 锁内一次性拷贝的纯数据视图
///
/// min-max / LTTB 等显示降采样在快照上锁外计算: debug 构建下 70k 点 × 4 通道的
/// 包络计算可达 20ms+, 若持锁计算会把摄入热路径 (push_frame) 饿死在锁竞争上
/// (广播积压溢出 → 大量丢帧 → 波形块状失真)。
#[derive(Debug, Clone)]
pub struct WindowSnapshot {
    /// 快照锚定的绝对最新时间戳 (微秒)
    latest_us: u64,
    /// 窗口内绝对时间戳 (微秒), 下标即局部序号; 其长度即降采样前的原始点数
    timestamps: Vec<u64>,
    /// 选中序列 (通道/派生)
    series: Vec<SnapshotSeries>,
    /// 通道槽总数 (WaveformWindow.channels 保持完整通道槽形状)
    num_channels: usize,
    /// 后端波形缓冲区当前点数/容量 (状态栏缓存使用率)
    buffer_points: usize,
    buffer_capacity: usize,
    /// 是否来自金字塔层 (真实 min-max 包络; 采样标志强制非 Raw)
    from_tier: bool,
    /// 金字塔层序号 (from_tier 时的层 k, 0 基; raw 路径无意义)
    tier_level: u8,
    /// L0 滚动覆盖累计 (快照锚定时刻)
    storage_overflow: u64,
    /// 降采样前落在请求时间窗内的原始点数 (层服务时含已被 L0 覆盖丢弃的部分)
    raw_window_points: usize,
}

/// 波形数据窗口 — 供前端查询
#[derive(Debug, Clone, Serialize)]
pub struct WaveformWindow {
    /// 组级单调序号 — 分片并发推送时前端按 "最新 seq 胜出" 丢弃旧快照
    #[serde(default)]
    pub seq: u64,
    /// 时间戳数组 (相对最新的偏移, 单位: 毫秒)
    pub timestamps: Vec<f64>,
    /// 每通道的数据数组
    pub channels: Vec<Vec<f32>>,
    /// 当前检测到的通道数
    pub channel_count: usize,
    /// 派生通道数据 (Math/Filter 等节点的输出, 作为 Waveform sink 的输入)
    /// key1 = sink_widget_id, key2 = source_widget_id, key3 = source_handle
    #[serde(default)]
    pub derived: HashMap<String, HashMap<String, HashMap<String, Vec<f32>>>>,
    /// 后端波形缓冲区当前点数 (用于状态栏显示缓存使用率)
    #[serde(default)]
    pub buffer_points: usize,
    /// 后端波形缓冲区最大容量 (点)
    #[serde(default)]
    pub buffer_capacity: usize,
    /// 本窗口所锚定的绝对最新时间戳（微秒）。
    #[serde(default)]
    pub latest_timestamp_us: u64,
    /// 降采样前落在请求时间窗内的原始点数。
    #[serde(default)]
    pub raw_window_points: usize,
    /// 窗口采用的采样策略。
    #[serde(default)]
    pub sampling: WaveformSampling,
    /// L0 滚动覆盖丢弃的样本累计数 (降载徽标; 0 = 原始层完整覆盖当前窗口)
    #[serde(default)]
    pub storage_overflow: u64,
    /// 服务本窗口的金字塔层级 (0 = 原始层; k>0 = min-max 第 k 层, 降载显示)
    #[serde(default)]
    pub buffer_tier: u8,
}
