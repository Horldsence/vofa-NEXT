//! WindowSnapshot 降采样 — min-max 包络 / LTTB (锁外计算)

use std::collections::HashMap;

use crate::DataBuffer;

use super::{SeriesTarget, SnapshotSeries, WaveformSampling, WaveformWindow, WindowSnapshot};

impl WindowSnapshot {
    /// 降采样前窗口内的原始点数
    pub const fn raw_window_points(&self) -> usize {
        self.raw_window_points
    }

    /// 由选中局部序号构建输出窗口 (相对时间戳 + 完整通道槽形状)
    fn build_window(&self, selected: &[usize], sampling: WaveformSampling) -> WaveformWindow {
        let timestamps: Vec<f64> = selected
            .iter()
            .map(|&index| DataBuffer::relative_timestamp_ms(self.timestamps[index], self.latest_us))
            .collect();
        let mut channels = vec![Vec::new(); self.num_channels];
        let mut derived: HashMap<String, HashMap<String, HashMap<String, Vec<f32>>>> =
            HashMap::new();
        for series in &self.series {
            let values: Vec<f32> = selected.iter().map(|&index| series.values[index]).collect();
            match &series.target {
                SeriesTarget::Channel(channel) => channels[*channel] = values,
                SeriesTarget::Derived {
                    sink,
                    source,
                    handle,
                } => {
                    derived
                        .entry(sink.clone())
                        .or_default()
                        .entry(source.clone())
                        .or_default()
                        .insert(handle.clone(), values);
                }
            }
        }
        let sampling = if self.from_tier || self.raw_window_points > timestamps.len() {
            sampling
        } else {
            WaveformSampling::Raw
        };
        WaveformWindow {
            seq: 0,
            timestamps,
            channels,
            channel_count: self.num_channels,
            derived,
            buffer_points: self.buffer_points,
            buffer_capacity: self.buffer_capacity,
            latest_timestamp_us: self.latest_us,
            raw_window_points: self.raw_window_points,
            sampling,
            storage_overflow: self.storage_overflow,
            buffer_tier: if self.from_tier {
                self.tier_level.saturating_add(1)
            } else {
                0
            },
        }
    }

    /// 实时逐像素峰谷包络 (锁外计算)。多序列共享 X 轴，因此每个桶会合并所有
    /// 序列的 min/max 原始序号。若调用方预算小到连一个桶的全部峰谷都容纳不下，
    /// 峰值完整性优先，返回点数可超过该软预算。
    pub fn into_min_max(self, max_points: usize) -> WaveformWindow {
        let raw_points = self.timestamps.len();
        if raw_points == 0 {
            return self.build_window(&[], WaveformSampling::MinMax);
        }
        let limit = max_points.max(2);
        if self.series.is_empty() {
            let selected = DataBuffer::uniform_indices(0, raw_points, limit);
            return self.build_window(&selected, WaveformSampling::MinMax);
        }
        let minimum_extrema_budget = 2usize.saturating_add(self.series.len().saturating_mul(2));
        let effective_limit = limit.max(minimum_extrema_budget);
        if raw_points <= effective_limit {
            return self.build_window(
                &(0..raw_points).collect::<Vec<_>>(),
                WaveformSampling::MinMax,
            );
        }
        let bucket_count = ((effective_limit - 2) / (self.series.len() * 2))
            .max(1)
            .min(raw_points);
        let mut selected = Vec::with_capacity(effective_limit);
        selected.push(0);

        for bucket in 0..bucket_count {
            let bucket_start = bucket * raw_points / bucket_count;
            let bucket_end = ((bucket + 1) * raw_points / bucket_count).min(raw_points);
            for series in &self.series {
                let mut min: Option<(usize, f32)> = None;
                let mut max: Option<(usize, f32)> = None;
                for index in bucket_start..bucket_end {
                    let value = series.values[index];
                    if !value.is_finite() {
                        continue;
                    }
                    if min.is_none_or(|(_, current)| value < current) {
                        min = Some((index, value));
                    }
                    if max.is_none_or(|(_, current)| value > current) {
                        max = Some((index, value));
                    }
                }
                if let Some((index, _)) = min {
                    selected.push(index);
                }
                if let Some((index, _)) = max {
                    selected.push(index);
                }
            }
        }
        selected.push(raw_points - 1);
        selected.sort_unstable();
        selected.dedup();
        self.build_window(&selected, WaveformSampling::MinMax)
    }

    /// 停止态 LTTB 视觉采样 (锁外计算)
    pub fn into_lttb(self, max_points: usize) -> WaveformWindow {
        let raw_points = self.timestamps.len();
        if raw_points == 0 {
            return self.build_window(&[], WaveformSampling::Lttb);
        }
        let limit = max_points.max(3);
        if self.series.is_empty() {
            let selected = DataBuffer::uniform_indices(0, raw_points, limit);
            return self.build_window(&selected, WaveformSampling::Lttb);
        }
        let effective_limit = limit.max(2usize.saturating_add(self.series.len()));
        if raw_points <= effective_limit {
            return self.build_window(&(0..raw_points).collect::<Vec<_>>(), WaveformSampling::Lttb);
        }
        let per_series_interior = ((effective_limit - 2) / self.series.len()).max(1);
        let threshold = per_series_interior.saturating_add(2).min(raw_points);
        let mut selected = Vec::with_capacity(effective_limit);
        for series in &self.series {
            selected.extend(self.lttb_series_indices(threshold, series));
        }
        selected.sort_unstable();
        selected.dedup();
        self.build_window(&selected, WaveformSampling::Lttb)
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn lttb_series_indices(&self, threshold: usize, series: &SnapshotSeries) -> Vec<usize> {
        let points = self.timestamps.len();
        if points <= threshold || threshold < 3 {
            return (0..points).collect();
        }

        let every = (points - 2) as f64 / (threshold - 2) as f64;
        let origin = self.timestamps[0];
        let x = |index: usize| self.timestamps[index].saturating_sub(origin) as f64;
        let mut selected = Vec::with_capacity(threshold);
        selected.push(0);
        let mut anchor = 0usize;

        for bucket in 0..threshold - 2 {
            let average_start = ((((bucket + 1) as f64 * every).floor() as usize) + 1).min(points);
            let average_end = ((((bucket + 2) as f64 * every).floor() as usize) + 1).min(points);
            let mut average_x = 0.0;
            let mut average_y = 0.0;
            let mut average_count = 0usize;
            for index in average_start..average_end {
                let value = series.values[index];
                if value.is_finite() {
                    average_x += x(index);
                    average_y += f64::from(value);
                    average_count += 1;
                }
            }
            if average_count > 0 {
                average_x /= average_count as f64;
                average_y /= average_count as f64;
            } else {
                average_x = x(average_start.min(points - 1));
                average_y = f64::from(series.values[anchor]);
                if !average_y.is_finite() {
                    average_y = 0.0;
                }
            }

            let range_start = (((bucket as f64 * every).floor() as usize) + 1).min(points - 1);
            let range_end = ((((bucket + 1) as f64 * every).floor() as usize) + 1)
                .min(points - 1)
                .max(range_start + 1);
            let anchor_x = x(anchor);
            let mut anchor_y = f64::from(series.values[anchor]);
            if !anchor_y.is_finite() {
                anchor_y = 0.0;
            }
            let mut best: Option<(usize, f64)> = None;
            for index in range_start..range_end.min(points - 1) {
                let value = series.values[index];
                if !value.is_finite() {
                    continue;
                }
                let candidate_x = x(index);
                let candidate_y = f64::from(value);
                let area = (anchor_x - candidate_x)
                    .mul_add(
                        -(average_y - anchor_y),
                        (anchor_x - average_x) * (candidate_y - anchor_y),
                    )
                    .abs();
                if best.is_none_or(|(_, current)| area > current) {
                    best = Some((index, area));
                }
            }
            if let Some((index, _)) = best {
                selected.push(index);
                anchor = index;
            }
        }
        selected.push(points - 1);
        selected
    }
}
