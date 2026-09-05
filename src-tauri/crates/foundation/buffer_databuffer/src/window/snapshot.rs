//! DataBuffer 快照族 — 锁内短临界区拷贝 + 金字塔分层预算选择

use std::collections::HashSet;

use crate::DataBuffer;

use super::{
    SeriesTarget, SnapshotSeries, WaveformSeriesSelection, WaveformWindow, WindowSnapshot,
};

impl DataBuffer {
    /// 锁内一次性拷贝 [start, end) 窗口的纯数据快照 (拷贝量级 = 窗口点数,
    /// 微秒级 memcpy), 供锁外降采样计算
    fn snapshot_range(
        &self,
        start: usize,
        end: usize,
        latest_us: u64,
        selection: &WaveformSeriesSelection,
    ) -> WindowSnapshot {
        let total = self.timestamps.len();
        let timestamps: Vec<u64> = (start..end)
            .filter_map(|index| self.timestamps.get(index).copied())
            .collect();
        let mut series = Vec::new();
        let mut seen_channels = HashSet::new();
        for &channel in &selection.channels {
            if channel < self.num_channels && seen_channels.insert(channel) {
                let values = (start..end)
                    .map(|index| self.channel_value(channel, index, total))
                    .collect();
                series.push(SnapshotSeries {
                    target: SeriesTarget::Channel(channel),
                    values,
                });
            }
        }
        {
            // 单次加锁完成筛选 + 时间对齐取值 (派生锁不可重入, 锁内不得再调
            // selected_derived_indices 等加锁方法)
            let store = self.derived.lock();
            for (derived_index, entry) in store.entries().iter().enumerate() {
                if entry.rb.is_empty()
                    || !selection.derived.iter().any(|selected| {
                        selected.sink_id == entry.sink
                            && selected.source_id == entry.source
                            && selected.source_handle == entry.source_handle
                    })
                {
                    continue;
                }
                let values = store.values_at_timestamps(derived_index, &timestamps);
                series.push(SnapshotSeries {
                    target: SeriesTarget::Derived {
                        sink: entry.sink.clone(),
                        source: entry.source.clone(),
                        handle: entry.source_handle.clone(),
                    },
                    values,
                });
            }
        }
        WindowSnapshot {
            latest_us,
            raw_window_points: timestamps.len(),
            timestamps,
            series,
            num_channels: self.num_channels,
            buffer_points: total,
            buffer_capacity: self.max_points,
            from_tier: false,
            tier_level: 0,
            storage_overflow: self.storage_overflow,
        }
    }

    /// 按相对最新时间的浮点毫秒范围拷贝窗口快照 (锁内短临界区)
    pub fn snapshot_window(
        &self,
        start_ms: f64,
        end_ms: f64,
        selection: &WaveformSeriesSelection,
    ) -> WindowSnapshot {
        let (start, end, latest) = self.range_indices(start_ms, end_ms);
        self.snapshot_range(start, end, latest, selection)
    }

    /// 全缓冲快照 (概览流用, 覆盖所有序列)
    pub fn snapshot_all(&self) -> WindowSnapshot {
        let total = self.timestamps.len();
        let latest = self
            .timestamps
            .get(total.saturating_sub(1))
            .copied()
            .unwrap_or(0);
        self.snapshot_range(0, total, latest, &self.all_series_selection())
    }

    /// 分层预算快照 — 波形流消费入口 (不变量 2: 容量自洽, 示波器语义)
    ///
    /// 窗口完整落在 L0 覆盖内且原始点数 ≤ 4×budget → 原始快照; 否则自动选择
    /// 最小的金字塔层 (窗口内条目数 ≤ 4×budget), 提供**真实 min-max 包络**。
    /// 窗口起点早于 L0 最旧样本时 (超出原始覆盖), 旧段由金字塔层补全 —
    /// 旧数据降质不消失。
    pub fn snapshot_window_budget(
        &self,
        start_ms: f64,
        end_ms: f64,
        selection: &WaveformSeriesSelection,
        budget: usize,
    ) -> WindowSnapshot {
        let (start, end, latest) = self.range_indices(start_ms, end_ms);
        let raw_count = end.saturating_sub(start);
        let from_ts = Self::timestamp_at_offset(latest, start_ms.min(end_ms));
        let to_ts = Self::timestamp_at_offset(latest, start_ms.max(end_ms));
        let beyond_l0 = self.timestamps.len() >= self.max_points
            && self
                .timestamps
                .get(0)
                .is_some_and(|oldest| from_ts < *oldest);
        if !beyond_l0 && raw_count <= budget.saturating_mul(4) {
            return self.snapshot_range(start, end, latest, selection);
        }
        // 自底向上找"覆盖窗口起点且条目数 ≤ 4×budget"的最小层；没有层能
        // 严格覆盖时，回退到实际保留了最早时间的层。
        let mut fallback: Option<(usize, u64)> = None;
        for k in 0..self.tiers.len() {
            if self.tiers[k].entry_count() == 0 {
                continue;
            }
            let Some(oldest) = self.tiers[k].oldest_ts() else {
                continue;
            };
            let tier_fits = oldest <= from_ts;
            let (lo, hi) = self.tiers[k].range_bounds(from_ts, to_ts);
            if tier_fits && hi.saturating_sub(lo) <= budget.saturating_mul(4) {
                return self.snapshot_tier(k, from_ts, to_ts, latest, selection);
            }
            // 块时间戳取块末，因此请求恰好从数据起点开始时，所有层的 oldest
            // 都可能略晚于 from_ts。无完整覆盖层时选择实际最早的层；固定容量
            // 环发生覆盖后，它通常会自然落到跨度更大的高层。
            if fallback.is_none_or(|(_, fallback_oldest)| oldest < fallback_oldest) {
                fallback = Some((k, oldest));
            }
        }
        fallback.map_or_else(
            || self.snapshot_range(start, end, latest, selection),
            |(k, _)| self.snapshot_tier(k, from_ts, to_ts, latest, selection),
        )
    }

    /// 全历史预算快照 (概览流) — 从能装下预算的最小层取全历史包络
    pub fn snapshot_all_budget(&self, budget: usize) -> WindowSnapshot {
        let latest = self
            .timestamps
            .get(self.timestamps.len().saturating_sub(1))
            .copied()
            .unwrap_or(0);
        // 从细到粗选择第一个满足预算的层。倒序会总命中刚生成、只有两个点的
        // 最粗层，概览随运行时间突然塌缩或看似消失。
        for k in 0..self.tiers.len() {
            let entries = self.tiers[k].entry_count();
            if entries > 0 && entries <= budget.saturating_mul(4) {
                return self.snapshot_tier_all(k, latest);
            }
        }
        self.tiers
            .iter()
            .rposition(|tier| tier.entry_count() > 0)
            .map_or_else(
                || self.snapshot_all(),
                |k| self.snapshot_tier_all(k, latest),
            )
    }

    /// 从第 `tier_ix` 层取窗口区间快照 (min-max 交错条目即真实包络)
    fn snapshot_tier(
        &self,
        tier_ix: usize,
        from_ts: u64,
        to_ts: u64,
        latest: u64,
        selection: &WaveformSeriesSelection,
    ) -> WindowSnapshot {
        let tier = &self.tiers[tier_ix];
        let (lo, hi) = tier.range_bounds(from_ts, to_ts);
        let timestamps: Vec<u64> = (lo..hi)
            .filter_map(|i| tier.timestamps.get(i).copied())
            .collect();
        let mut series = Vec::new();
        let mut seen_channels = HashSet::new();
        for &channel in &selection.channels {
            if channel < tier.series.len() && seen_channels.insert(channel) {
                let s = &tier.series[channel];
                let values = (lo..hi)
                    .map(|i| s.val.get(i).copied().unwrap_or(f32::NAN))
                    .collect();
                series.push(SnapshotSeries {
                    target: SeriesTarget::Channel(channel),
                    values,
                });
            }
        }
        {
            let store = self.derived.lock();
            for (derived_index, entry) in store.entries().iter().enumerate() {
                if entry.rb.is_empty()
                    || !selection.derived.iter().any(|selected| {
                        selected.sink_id == entry.sink
                            && selected.source_id == entry.source
                            && selected.source_handle == entry.source_handle
                    })
                {
                    continue;
                }
                let values = store.values_at_timestamps(derived_index, &timestamps);
                series.push(SnapshotSeries {
                    target: SeriesTarget::Derived {
                        sink: entry.sink.clone(),
                        source: entry.source.clone(),
                        handle: entry.source_handle.clone(),
                    },
                    values,
                });
            }
        }
        // 窗口真实原始点数按层折算 (条目对数 × 16^(k+1)); 金字塔层服务的
        // 窗口原始点数远大于本快照点数 → sampling 标 MinMax
        let factor = 16_u64.pow(u32::try_from(tier_ix).unwrap_or(0) + 1);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let raw_window_points = hi.saturating_sub(lo).saturating_add(1) / 2 * factor as usize;
        WindowSnapshot {
            latest_us: latest,
            timestamps,
            series,
            num_channels: self.num_channels,
            buffer_points: self.timestamps.len(),
            buffer_capacity: self.max_points,
            from_tier: true,
            tier_level: u8::try_from(tier_ix).unwrap_or(u8::MAX),
            storage_overflow: self.storage_overflow,
            raw_window_points,
        }
    }

    /// 从第 `tier_ix` 层取全历史快照
    fn snapshot_tier_all(&self, tier_ix: usize, latest: u64) -> WindowSnapshot {
        let tier = &self.tiers[tier_ix];
        let from = tier.timestamps.get(0).copied().unwrap_or(0);
        self.snapshot_tier(
            tier_ix,
            from,
            u64::MAX,
            latest,
            &self.all_series_selection(),
        )
    }

    pub(super) fn uniform_indices(start: usize, end: usize, limit: usize) -> Vec<usize> {
        let points = end.saturating_sub(start);
        if points <= limit {
            return (start..end).collect();
        }
        let last = points - 1;
        (0..limit)
            .map(|index| start + index * last / (limit - 1))
            .collect()
    }

    /// 获取覆盖整个历史的实时峰谷概览。
    pub fn get_min_max(&self, max_points: usize) -> WaveformWindow {
        self.snapshot_all().into_min_max(max_points)
    }

    /// 按相对最新时间的浮点毫秒范围生成实时峰谷包络。
    pub fn get_window_min_max(
        &self,
        start_ms: f64,
        end_ms: f64,
        max_points: usize,
        selection: &WaveformSeriesSelection,
    ) -> WaveformWindow {
        self.snapshot_window(start_ms, end_ms, selection)
            .into_min_max(max_points)
    }

    /// 按相对最新时间的浮点毫秒范围生成停止态 LTTB 视觉采样。
    pub fn get_window_lttb(
        &self,
        start_ms: f64,
        end_ms: f64,
        max_points: usize,
        selection: &WaveformSeriesSelection,
    ) -> WaveformWindow {
        self.snapshot_window(start_ms, end_ms, selection)
            .into_lttb(max_points)
    }
}
