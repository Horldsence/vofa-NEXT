//! min-max 金字塔 — DataBuffer 的分层降载结构 (不变量 2)
//!
//! 记录路径逐样本追加 L0; 每凑满 [`DECIMATION`] 个样本折叠一个块摘要进 L1,
//! L1 凑满 16 块再折叠进 L2, 依此级联 (摊销 O(1)/样本, 内存 ≈ 原始层的 7%)。
//! L0 环覆盖不住的窗口由更高层提供**真实 min-max 包络** — 旧数据"降质不消失",
//! 任意缩放级别的查询都是该分辨率下的真包络, 而不是抽点/缝合的近似。
//!
//! 层条目布局: 每块使用一对相同的公共时间戳 (块末时间), 每通道保存
//! `(min, max)` 两个值。公共时间轴因此始终单调且通道间严格对齐; 不能把某一
//! 通道各自的极值时刻借给其他通道作为 X 坐标。`pushed` 为逻辑块计数
//! (环形覆盖不改写计数)。

use buffer_ring::RingBuffer;

use crate::DataBuffer;

/// 相邻层的降采样倍数 (每块样本数 / 每块上层条目数)
const DECIMATION: u64 = 16;

/// 每层固定条目容量 (min/max 交错对数) — 各层等容量意味着时间跨度按
/// [`DECIMATION`] 几何级增长: L1 ≈ 6.5 万样本, L2 ≈ 100 万, L3 ≈ 1600 万…
/// 内存每层仅 ~100KB/通道, 深历史以粗分辨率"降质不消失"。
const TIER_CAPACITY_ENTRIES: usize = 4_096;

/// 单层单通道序列 — 每块一对交错条目
#[derive(Clone)]
pub struct TierSeries {
    /// 交错值 (min, max)
    pub(crate) val: RingBuffer<f32>,
}

impl TierSeries {
    fn new() -> Self {
        Self {
            val: RingBuffer::new(TIER_CAPACITY_ENTRIES.saturating_mul(2)),
        }
    }

    fn push_pair(&mut self, min: f32, max: f32) {
        self.val.push(min);
        self.val.push(max);
    }
}

/// 单个金字塔层
#[derive(Clone)]
pub struct Tier {
    /// 所有通道共享的单调时间轴。每块写入两次块末时间, 对应各通道的 min/max。
    pub(crate) timestamps: RingBuffer<u64>,
    pub(crate) series: Vec<TierSeries>,
    /// 已折叠进本层的块数 (逻辑计数)
    pushed: u64,
}

impl Tier {
    /// 已分配的数值列与公共时间轴容量，不含 Vec 元数据和分配器开销。
    pub(crate) const fn estimated_bytes(&self) -> usize {
        TIER_CAPACITY_ENTRIES
            .saturating_mul(2)
            .saturating_mul(8 + 4 * self.series.len())
    }

    fn new(channels: usize) -> Self {
        Self {
            timestamps: RingBuffer::new(TIER_CAPACITY_ENTRIES.saturating_mul(2)),
            series: (0..channels).map(|_| TierSeries::new()).collect(),
            pushed: 0,
        }
    }

    /// 本层最旧条目的时间戳 (空层返回 None) — 窗口覆盖判定用
    pub fn oldest_ts(&self) -> Option<u64> {
        self.timestamps.get(0).copied()
    }

    /// 层内当前条目数 (各通道对齐, 取通道 0)
    pub const fn entry_count(&self) -> usize {
        self.timestamps.len()
    }

    /// 时间戳环上 [from_ts, to_ts] 的条目区间 (二分; 各通道对齐, 取通道 0)
    pub fn range_bounds(&self, from_ts: u64, to_ts: u64) -> (usize, usize) {
        let len = self.timestamps.len();
        // lower bound: 首个 >= from_ts
        let mut lo = 0_usize;
        let mut hi = len;
        while lo < hi {
            let mid = usize::midpoint(lo, hi);
            if self.timestamps.get(mid).is_some_and(|t| *t < from_ts) {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        // upper bound: 首个 > to_ts
        let mut upper = lo;
        hi = len;
        while upper < hi {
            let mid = usize::midpoint(upper, hi);
            if self.timestamps.get(mid).is_some_and(|t| *t <= to_ts) {
                upper = mid + 1;
            } else {
                hi = mid;
            }
        }
        (lo, upper)
    }
}

/// 一个待写入层条目的块摘要。时间戳取块末样本, 各通道共享；值为真实 min/max。
struct Block {
    timestamp: u64,
    values: Vec<(f32, f32)>,
}

impl DataBuffer {
    /// 记录路径逐样本追加后调用: L0 凑满一个块则级联折叠 (摊销 O(1))
    pub(crate) fn maybe_fold(&mut self) {
        self.raw_pushed = self.raw_pushed.wrapping_add(1);
        if !self.raw_pushed.is_multiple_of(DECIMATION) {
            return;
        }
        let mut block = self.fold_block_from_raw();
        let mut level = 0_usize;
        loop {
            self.ensure_tier(level);
            {
                let tier = &mut self.tiers[level];
                tier.pushed += 1;
                tier.timestamps.push(block.timestamp);
                tier.timestamps.push(block.timestamp);
                for (ch, entry) in tier.series.iter_mut().enumerate() {
                    if let Some(&(min, max)) = block.values.get(ch) {
                        entry.push_pair(min, max);
                    }
                }
            }
            if !self.tiers[level].pushed.is_multiple_of(DECIMATION) {
                break;
            }
            block = Self::fold_block_from_tier(&self.tiers[level]);
            level += 1;
        }
    }

    /// 确保第 `level` 层存在 (固定条目容量, 跨度随层级几何增长)
    fn ensure_tier(&mut self, level: usize) {
        while self.tiers.len() <= level {
            self.tiers.push(Tier::new(self.num_channels));
        }
    }

    /// 从 L0 最近 `DECIMATION` 个样本合成一个块摘要
    fn fold_block_from_raw(&self) -> Block {
        let n = self.timestamps.len();
        #[allow(clippy::cast_possible_truncation)] // DECIMATION=16, 32 位平台也无截断
        let take = (DECIMATION as usize).min(n);
        let start = n - take;
        Block {
            timestamp: self
                .timestamps
                .get(n.saturating_sub(1))
                .copied()
                .unwrap_or(0),
            values: (0..self.num_channels)
                .map(|ch| self.min_max_over_range(ch, start..n))
                .collect(),
        }
    }

    /// 从某层最近 `DECIMATION` 个条目合成一个上层块摘要
    fn fold_block_from_tier(tier: &Tier) -> Block {
        Block {
            timestamp: tier
                .timestamps
                .get(tier.timestamps.len().saturating_sub(1))
                .copied()
                .unwrap_or(0),
            values: tier
                .series
                .iter()
                .map(|series| {
                    #[allow(clippy::cast_possible_truncation)] // DECIMATION=16, 无截断
                    let take = (DECIMATION as usize * 2).min(series.val.len());
                    let start = series.val.len() - take;
                    let mut min: Option<f32> = None;
                    let mut max: Option<f32> = None;
                    for i in start..series.val.len() {
                        let Some(v) = series.val.get(i).copied() else {
                            continue;
                        };
                        if !v.is_finite() {
                            continue;
                        }
                        if min.is_none_or(|m| v < m) {
                            min = Some(v);
                        }
                        if max.is_none_or(|m| v > m) {
                            max = Some(v);
                        }
                    }
                    match (min, max) {
                        (Some(min), Some(max)) => (min, max),
                        // 全 NaN 块保持时间轴占位
                        _ => (f32::NAN, f32::NAN),
                    }
                })
                .collect(),
        }
    }

    /// 区间 [start, end) 内的 min/max (有限值; 全 NaN 返回占位)
    fn min_max_over_range(&self, ch: usize, range: std::ops::Range<usize>) -> (f32, f32) {
        let n = self.timestamps.len();
        let mut min: Option<f32> = None;
        let mut max: Option<f32> = None;
        for i in range {
            let v = self.channel_value(ch, i, n);
            if !v.is_finite() {
                continue;
            }
            if min.is_none_or(|m| v < m) {
                min = Some(v);
            }
            if max.is_none_or(|m| v > m) {
                max = Some(v);
            }
        }
        match (min, max) {
            (Some(min), Some(max)) => (min, max),
            _ => (f32::NAN, f32::NAN),
        }
    }
}
