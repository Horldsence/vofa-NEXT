//! 后端测量引擎 — 权威缓冲 (金字塔快照) 上的统计/周期检测/自动设置
//!
//! 数据路径唯一: 一律经由 [`buffer_databuffer::DataBuffer::snapshot_window_budget`]
//! 取数 (窗口在 L0 内且原始点数 ≤ 4×budget → 原始快照; 否则金字塔自底向上选
//! 最小覆盖层) — 不新建任何降采样路径, 层级正确性由金字塔自身机制保证。
//!
//! 测量对象: 协议通道 + 派生序列 (MATH/Filter 等接入波形 sink 的派生输出) —
//! AutoSet 的时基由**所有被测序列中最慢的基波周期**驱动, 慢波形来自派生
//! 序列时同样参与检测 (快通道检测成功而慢派生失败会导致窗口过短的回归)。
//!
//! 层语义 (tier-aware):
//! - 层快照为交错 `(min, max)` 对 + 共享块末时间戳 (派生序列按层时间戳对齐
//!   采样, 每对两值); `dt` 一律从时间戳实测, 不按 16^(k+1) 假设;
//! - `vmin/vmax/vpp` 任何层都精确 (包络极值即真值); `vavg/vrms/duty` 在层上
//!   用对中点序列近似, 原始路径精确 — 载荷以 `from_tier/tier_level` 诚实标注;
//! - 周期检测带分辨率守卫: 仅接受 ≥ [`PERIOD_GUARD`]×dt 的周期, 粗层测不出
//!   快周期时返回 None (诚实拒绝, 不给混叠伪值); AutoSet 路径允许一次
//!   细化重查 (预算 ×4) 以扩展快周期量程。
//!
//! 所有计算函数均为同步纯计算 (锁内取快照、锁外算), 调用方须在
//! `tokio::task::block_in_place` / `spawn_blocking` 中执行, 不阻塞异步 worker。

use serde::Serialize;

/// 测量取数预算 — snapshot_window_budget 语义下最多取 4×budget 条目
/// (≈65k 条目 = 32k min/max 对或 65k 原始点; ACF 输入截断至 [`ACF_MAX_POINTS`])
const MEASURE_BUDGET: usize = 16_384;
/// AutoSet 细化重查预算 (×4 — 允许金字塔越一层, 扩展快周期量程)
const REFINE_BUDGET: usize = 65_536;
/// ACF 输入点数上限 — 超出截取最近 N 点 (等间隔无混叠, 仅缩短分析跨度)
const ACF_MAX_POINTS: usize = 131_072;
/// 周期分辨率守卫: 接受的周期必须 ≥ 6×采样间隔 (1-2-5 档位精度远低于此)
const PERIOD_GUARD: f64 = 6.0;
/// AutoSet 搜索窗口 (毫秒) = 最大时基档 5s × 10 div — 周期检测与回退拟合的范围
const AUTOSET_SEARCH_WINDOW_MS: f64 = 50_000.0;

/// 单通道测量结果 (频率/周期/占空比不可测为 null)
#[derive(Debug, Clone, Serialize)]
pub struct ChannelMeasurement {
    pub channel: usize,
    pub vpp: f64,
    pub vmin: f64,
    pub vmax: f64,
    pub vavg: f64,
    /// RMS (含直流)
    pub vrms: f64,
    /// RMS (去直流) — AC 耦合显示换算用
    pub vrms_ac: f64,
    pub duty: Option<f64>,
    pub freq: Option<f64>,
    pub period: Option<f64>,
}

/// 派生序列 (MATH/Filter 接入波形 sink) 测量结果 — 与通道同构统计 + 三元组键
#[derive(Debug, Clone, Serialize)]
pub struct DerivedMeasurement {
    pub sink_id: String,
    pub source_id: String,
    pub source_handle: String,
    pub vpp: f64,
    pub vmin: f64,
    pub vmax: f64,
    pub vavg: f64,
    pub vrms: f64,
    pub vrms_ac: f64,
    pub duty: Option<f64>,
    pub freq: Option<f64>,
    pub period: Option<f64>,
}

/// 单数据源测量快照 (JSON 推送; `from_tier` 诚实标注精度来源)
#[derive(Debug, Clone, Serialize)]
pub struct SourceMeasurements {
    pub source: String,
    /// 单调递增序号 — 前端按最新胜出
    pub seq: u64,
    pub window_ms: f64,
    pub latest_timestamp_us: u64,
    /// 快照来自金字塔层 (vavg/vrms 为包络中点近似)
    pub from_tier: bool,
    /// 金字塔层序号 (from_tier 时有效)
    pub tier_level: u8,
    pub channels: Vec<ChannelMeasurement>,
    pub derived: Vec<DerivedMeasurement>,
}

/// 被测序列标识 — 协议通道或派生序列三元组
#[derive(Debug, Clone)]
enum SeriesKey {
    Channel(usize),
    Derived {
        sink_id: String,
        source_id: String,
        source_handle: String,
    },
}

/// 快照窗口的序列视图 — 原始路径为逐样本; 层路径为交错 (min,max) 对
/// (派生序列按层时间戳对齐, 每对两值共享块末时间)
struct WindowSeries {
    key: SeriesKey,
    values: Vec<f32>,
}

struct ExtractedWindow {
    series: Vec<WindowSeries>,
    /// 相对最新时间戳 (毫秒, 升序); 层路径每对写入两次块末时间
    timestamps_ms: Vec<f64>,
    latest_us: u64,
    from_tier: bool,
    tier_level: u8,
    /// 数据实际时间跨度 (毫秒) — AutoSet 回退拟合用
    span_ms: f64,
}

impl ExtractedWindow {
    const fn is_tier(&self) -> bool {
        self.from_tier
    }

    /// dt (毫秒): 层路径为块对周期 (stride 2), 原始路径为样本间隔 (stride 1)
    fn dt_ms(&self) -> Option<f64> {
        let stride = if self.is_tier() { 2 } else { 1 };
        median_dt_ms(&self.timestamps_ms, stride)
    }
}

/// 单序列统计 + 守卫周期 (通道与派生共用; 层/原始路径语义见模块文档)
struct SeriesStats {
    vpp: f64,
    vmin: f64,
    vmax: f64,
    vavg: f64,
    vrms: f64,
    vrms_ac: f64,
    duty: Option<f64>,
    period: Option<f64>,
}

mod api;
mod extract;
mod stats;

#[cfg(test)]
mod tests;

pub use api::{compute_autoset_suggestion, compute_source_measurements};

use extract::{extract_window, median_dt_ms};
use stats::measure_series;
