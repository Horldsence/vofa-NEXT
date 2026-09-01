//! `DataBuffer` 窗口切片 — WaveformWindow 查询 (get_window / get_recent) 与 NaN 对齐
//!
//! 派生缓冲区创建较晚时, 窗口早期位置填 NaN (表示 "尚无数据"),
//! 保证 derived[i] 与 channels[ch][i] 严格按时间戳对齐。

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

use serde::{Deserialize, Serialize};

use crate::DataBuffer;

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

#[derive(Debug, Clone, Copy)]
enum SeriesRef {
    Channel(usize),
    Derived(usize),
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
}

impl DataBuffer {
    /// 当前环形缓存最旧/最新绝对时间戳（微秒）。
    pub fn time_bounds_us(&self) -> Option<(u64, u64)> {
        let oldest = self.timestamps.get(0).copied()?;
        let latest = self
            .timestamps
            .get(self.timestamps.len().saturating_sub(1))
            .copied()?;
        Some((oldest, latest))
    }

    fn relative_timestamp_ms(timestamp_us: u64, latest_us: u64) -> f64 {
        if timestamp_us <= latest_us {
            -((latest_us - timestamp_us) as f64) / 1000.0
        } else {
            ((timestamp_us - latest_us) as f64) / 1000.0
        }
    }

    /// 兼容查询：获取相对时间窗口内的全部原始点。
    pub fn get_window(&self, start_ms: i64, end_ms: i64) -> WaveformWindow {
        self.get_window_raw(start_ms as f64, end_ms as f64)
    }

    /// 兼容查询：获取最近 N 个原始点。
    pub fn get_recent(&self, count: usize) -> WaveformWindow {
        let end = self.timestamps.len();
        let start = end.saturating_sub(count);
        let latest = self
            .timestamps
            .get(end.saturating_sub(1))
            .copied()
            .unwrap_or(0);
        self.build_window(
            &(start..end).collect::<Vec<_>>(),
            latest,
            end.saturating_sub(start),
            &self.all_series_selection(),
            WaveformSampling::Raw,
        )
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn timestamp_at_offset(latest_us: u64, offset_ms: f64) -> u64 {
        let delta_us = (offset_ms * 1000.0).round();
        if delta_us < 0.0 {
            latest_us.saturating_sub((-delta_us) as u64)
        } else {
            latest_us.saturating_add(delta_us as u64)
        }
    }

    fn lower_bound_timestamp(&self, target: u64) -> usize {
        let mut lo = 0;
        let mut hi = self.timestamps.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.timestamps.get(mid).copied().unwrap_or(u64::MAX) < target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    fn upper_bound_timestamp(&self, target: u64) -> usize {
        let mut lo = 0;
        let mut hi = self.timestamps.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.timestamps.get(mid).copied().unwrap_or(u64::MAX) <= target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    fn range_indices(&self, start_ms: f64, end_ms: f64) -> (usize, usize, u64) {
        let total = self.timestamps.len();
        let latest_us = self
            .timestamps
            .get(total.saturating_sub(1))
            .copied()
            .unwrap_or(0);
        if total == 0 {
            return (0, 0, latest_us);
        }
        let (start_ms, end_ms) = if start_ms <= end_ms {
            (start_ms, end_ms)
        } else {
            (end_ms, start_ms)
        };
        let start_us = Self::timestamp_at_offset(latest_us, start_ms);
        let end_us = Self::timestamp_at_offset(latest_us, end_ms);
        (
            self.lower_bound_timestamp(start_us),
            self.upper_bound_timestamp(end_us),
            latest_us,
        )
    }

    fn absolute_range_indices(&self, start_us: u64, end_us: u64) -> (usize, usize, u64) {
        let latest_us = self
            .timestamps
            .get(self.timestamps.len().saturating_sub(1))
            .copied()
            .unwrap_or(0);
        let (start_us, end_us) = if start_us <= end_us {
            (start_us, end_us)
        } else {
            (end_us, start_us)
        };
        (
            self.lower_bound_timestamp(start_us),
            self.upper_bound_timestamp(end_us),
            latest_us,
        )
    }

    fn channel_value(&self, channel: usize, index: usize, total: usize) -> f32 {
        let Some(series) = self.channels.get(channel) else {
            return f32::NAN;
        };
        let offset = total.saturating_sub(series.len());
        if index < offset {
            f32::NAN
        } else {
            series.get(index - offset).copied().unwrap_or(f32::NAN)
        }
    }

    fn derived_value(&self, derived_index: usize, index: usize, total: usize) -> f32 {
        let Some(entry) = self.derived_list.get(derived_index) else {
            return f32::NAN;
        };
        let offset = total.saturating_sub(entry.rb.len());
        if index < offset {
            f32::NAN
        } else {
            entry.rb.get(index - offset).copied().unwrap_or(f32::NAN)
        }
    }

    fn selected_derived_indices(&self, selection: &WaveformSeriesSelection) -> Vec<usize> {
        self.derived_list
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                selection
                    .derived
                    .iter()
                    .any(|selected| {
                        selected.sink_id == entry.sink
                            && selected.source_id == entry.source
                            && selected.source_handle == entry.source_handle
                    })
                    .then_some(index)
            })
            .collect()
    }

    fn all_series_selection(&self) -> WaveformSeriesSelection {
        WaveformSeriesSelection {
            channels: (0..self.num_channels).collect(),
            derived: self
                .derived_list
                .iter()
                .map(|entry| DerivedSeriesSelector {
                    sink_id: entry.sink.clone(),
                    source_id: entry.source.clone(),
                    source_handle: entry.source_handle.clone(),
                })
                .collect(),
        }
    }

    fn build_window(
        &self,
        selected: &[usize],
        latest_us: u64,
        raw_window_points: usize,
        selection: &WaveformSeriesSelection,
        sampling: WaveformSampling,
    ) -> WaveformWindow {
        let total = self.timestamps.len();
        let timestamps: Vec<f64> = selected
            .iter()
            .filter_map(|&index| self.timestamps.get(index).copied())
            .map(|timestamp| Self::relative_timestamp_ms(timestamp, latest_us))
            .collect();
        let selected_channels = selection.channels.iter().copied().collect::<HashSet<_>>();
        let channels = (0..self.num_channels)
            .map(|channel| {
                if !selected_channels.contains(&channel) {
                    return Vec::new();
                }
                selected
                    .iter()
                    .map(|&index| self.channel_value(channel, index, total))
                    .collect()
            })
            .collect();
        let selected_derived = self
            .selected_derived_indices(selection)
            .into_iter()
            .collect::<HashSet<_>>();
        let mut derived: HashMap<String, HashMap<String, HashMap<String, Vec<f32>>>> =
            HashMap::new();
        for (derived_index, entry) in self.derived_list.iter().enumerate() {
            if entry.rb.is_empty() || !selected_derived.contains(&derived_index) {
                continue;
            }
            let values = selected
                .iter()
                .map(|&index| self.derived_value(derived_index, index, total))
                .collect();
            derived
                .entry(entry.sink.clone())
                .or_default()
                .entry(entry.source.clone())
                .or_default()
                .insert(entry.source_handle.clone(), values);
        }
        let sampling = if raw_window_points > timestamps.len() {
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
            buffer_points: total,
            buffer_capacity: self.max_points,
            latest_timestamp_us: latest_us,
            raw_window_points,
            sampling,
        }
    }

    fn selected_series(&self, selection: &WaveformSeriesSelection) -> Vec<SeriesRef> {
        let mut seen_channels = HashSet::new();
        let channels = selection.channels.iter().copied().filter_map(|channel| {
            (channel < self.num_channels && seen_channels.insert(channel))
                .then_some(SeriesRef::Channel(channel))
        });
        let derived = self
            .selected_derived_indices(selection)
            .into_iter()
            .map(SeriesRef::Derived);
        channels.chain(derived).collect()
    }

    fn series_value(&self, series: SeriesRef, index: usize) -> f32 {
        match series {
            SeriesRef::Channel(channel) => {
                self.channel_value(channel, index, self.timestamps.len())
            }
            SeriesRef::Derived(derived) => {
                self.derived_value(derived, index, self.timestamps.len())
            }
        }
    }

    fn uniform_indices(start: usize, end: usize, limit: usize) -> Vec<usize> {
        let points = end.saturating_sub(start);
        if points <= limit {
            return (start..end).collect();
        }
        let last = points - 1;
        (0..limit)
            .map(|index| start + index * last / (limit - 1))
            .collect()
    }

    /// 实时逐像素峰谷包络。多序列共享 X 轴，因此每个桶会合并所有可见序列的
    /// min/max 原始索引。若调用方预算小到连一个桶的全部峰谷都容纳不下，峰值
    /// 完整性优先，返回点数可超过该软预算。
    fn get_min_max_range(
        &self,
        start: usize,
        end: usize,
        latest_us: u64,
        max_points: usize,
        selection: &WaveformSeriesSelection,
    ) -> WaveformWindow {
        let raw_points = end.saturating_sub(start);
        if raw_points == 0 {
            return self.build_window(&[], latest_us, 0, selection, WaveformSampling::MinMax);
        }
        let limit = max_points.max(2);
        let series = self.selected_series(selection);
        if series.is_empty() {
            let selected = Self::uniform_indices(start, end, limit);
            return self.build_window(
                &selected,
                latest_us,
                raw_points,
                selection,
                WaveformSampling::MinMax,
            );
        }
        let minimum_extrema_budget = 2usize.saturating_add(series.len().saturating_mul(2));
        let effective_limit = limit.max(minimum_extrema_budget);
        if raw_points <= effective_limit {
            return self.build_window(
                &(start..end).collect::<Vec<_>>(),
                latest_us,
                raw_points,
                selection,
                WaveformSampling::MinMax,
            );
        }
        let bucket_count = ((effective_limit - 2) / (series.len() * 2))
            .max(1)
            .min(raw_points);
        let mut selected = Vec::with_capacity(effective_limit);
        selected.push(start);

        for bucket in 0..bucket_count {
            let bucket_start = start + bucket * raw_points / bucket_count;
            let bucket_end = start + ((bucket + 1) * raw_points / bucket_count).min(raw_points);
            for &series in &series {
                let mut min: Option<(usize, f32)> = None;
                let mut max: Option<(usize, f32)> = None;
                for index in bucket_start..bucket_end {
                    let value = self.series_value(series, index);
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
        selected.push(end - 1);
        selected.sort_unstable();
        selected.dedup();
        self.build_window(
            &selected,
            latest_us,
            raw_points,
            selection,
            WaveformSampling::MinMax,
        )
    }

    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    fn lttb_series_indices(
        &self,
        start: usize,
        end: usize,
        threshold: usize,
        series: SeriesRef,
    ) -> Vec<usize> {
        let points = end.saturating_sub(start);
        if points <= threshold || threshold < 3 {
            return (start..end).collect();
        }

        let every = (points - 2) as f64 / (threshold - 2) as f64;
        let origin = self.timestamps.get(start).copied().unwrap_or(0);
        let x = |index: usize| {
            self.timestamps
                .get(index)
                .copied()
                .unwrap_or(origin)
                .saturating_sub(origin) as f64
        };
        let mut selected = Vec::with_capacity(threshold);
        selected.push(start);
        let mut anchor = start;

        for bucket in 0..threshold - 2 {
            let average_start =
                (start + (((bucket + 1) as f64 * every).floor() as usize) + 1).min(end);
            let average_end =
                (start + (((bucket + 2) as f64 * every).floor() as usize) + 1).min(end);
            let mut average_x = 0.0;
            let mut average_y = 0.0;
            let mut average_count = 0usize;
            for index in average_start..average_end {
                let value = self.series_value(series, index);
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
                average_x = x(average_start.min(end - 1));
                average_y = f64::from(self.series_value(series, anchor));
                if !average_y.is_finite() {
                    average_y = 0.0;
                }
            }

            let range_start = (start + ((bucket as f64 * every).floor() as usize) + 1).min(end - 1);
            let range_end = (start + (((bucket + 1) as f64 * every).floor() as usize) + 1)
                .min(end - 1)
                .max(range_start + 1);
            let anchor_x = x(anchor);
            let mut anchor_y = f64::from(self.series_value(series, anchor));
            if !anchor_y.is_finite() {
                anchor_y = 0.0;
            }
            let mut best: Option<(usize, f64)> = None;
            for index in range_start..range_end.min(end - 1) {
                let value = self.series_value(series, index);
                if !value.is_finite() {
                    continue;
                }
                let candidate_x = x(index);
                let candidate_y = f64::from(value);
                let area = ((anchor_x - average_x) * (candidate_y - anchor_y)
                    - (anchor_x - candidate_x) * (average_y - anchor_y))
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
        selected.push(end - 1);
        selected
    }

    fn get_lttb_range(
        &self,
        start: usize,
        end: usize,
        latest_us: u64,
        max_points: usize,
        selection: &WaveformSeriesSelection,
    ) -> WaveformWindow {
        let raw_points = end.saturating_sub(start);
        if raw_points == 0 {
            return self.build_window(&[], latest_us, 0, selection, WaveformSampling::Lttb);
        }
        let limit = max_points.max(3);
        let series = self.selected_series(selection);
        if series.is_empty() {
            let selected = Self::uniform_indices(start, end, limit);
            return self.build_window(
                &selected,
                latest_us,
                raw_points,
                selection,
                WaveformSampling::Lttb,
            );
        }
        let effective_limit = limit.max(2usize.saturating_add(series.len()));
        if raw_points <= effective_limit {
            return self.build_window(
                &(start..end).collect::<Vec<_>>(),
                latest_us,
                raw_points,
                selection,
                WaveformSampling::Lttb,
            );
        }
        let per_series_interior = ((effective_limit - 2) / series.len()).max(1);
        let threshold = per_series_interior.saturating_add(2).min(raw_points);
        let mut selected = Vec::with_capacity(effective_limit);
        for series in series {
            selected.extend(self.lttb_series_indices(start, end, threshold, series));
        }
        selected.sort_unstable();
        selected.dedup();
        self.build_window(
            &selected,
            latest_us,
            raw_points,
            selection,
            WaveformSampling::Lttb,
        )
    }

    /// 获取覆盖整个历史的实时峰谷概览。
    pub fn get_min_max(&self, max_points: usize) -> WaveformWindow {
        let total = self.timestamps.len();
        let latest = self
            .timestamps
            .get(total.saturating_sub(1))
            .copied()
            .unwrap_or(0);
        self.get_min_max_range(0, total, latest, max_points, &self.all_series_selection())
    }

    /// 按相对最新时间的浮点毫秒范围生成实时峰谷包络。
    pub fn get_window_min_max(
        &self,
        start_ms: f64,
        end_ms: f64,
        max_points: usize,
        selection: &WaveformSeriesSelection,
    ) -> WaveformWindow {
        let (start, end, latest) = self.range_indices(start_ms, end_ms);
        self.get_min_max_range(start, end, latest, max_points, selection)
    }

    /// 按相对最新时间的浮点毫秒范围生成停止态 LTTB 视觉采样。
    pub fn get_window_lttb(
        &self,
        start_ms: f64,
        end_ms: f64,
        max_points: usize,
        selection: &WaveformSeriesSelection,
    ) -> WaveformWindow {
        let (start, end, latest) = self.range_indices(start_ms, end_ms);
        self.get_lttb_range(start, end, latest, max_points, selection)
    }

    /// 按相对最新时间返回全部原始点，不执行显示降采样。
    pub fn get_window_raw(&self, start_ms: f64, end_ms: f64) -> WaveformWindow {
        let (start, end, latest) = self.range_indices(start_ms, end_ms);
        let selection = self.all_series_selection();
        self.build_window(
            &(start..end).collect::<Vec<_>>(),
            latest,
            end.saturating_sub(start),
            &selection,
            WaveformSampling::Raw,
        )
    }

    /// 按绝对微秒时间范围返回所选原始序列，未选通道保留空槽以维持通道索引。
    pub fn get_window_raw_absolute_selected(
        &self,
        start_us: u64,
        end_us: u64,
        selection: &WaveformSeriesSelection,
    ) -> WaveformWindow {
        let (start, end, latest) = self.absolute_range_indices(start_us, end_us);
        self.build_window(
            &(start..end).collect::<Vec<_>>(),
            latest,
            end.saturating_sub(start),
            selection,
            WaveformSampling::Raw,
        )
    }

    /// 将绝对时间范围内的原始样本逐行写出 CSV，不构造百万点 IPC/JSON 对象。
    pub fn write_raw_csv<W: Write>(
        &self,
        writer: &mut W,
        start_us: u64,
        end_us: u64,
        selection: &WaveformSeriesSelection,
    ) -> io::Result<usize> {
        let (start, end, _) = self.absolute_range_indices(start_us, end_us);
        let mut seen_channels = HashSet::new();
        let channels: Vec<usize> = selection
            .channels
            .iter()
            .copied()
            .filter(|channel| *channel < self.num_channels && seen_channels.insert(*channel))
            .collect();
        let derived = self.selected_derived_indices(selection);

        write!(writer, "timestamp_us")?;
        for channel in &channels {
            write!(writer, ",CH{channel}")?;
        }
        for &derived_index in &derived {
            let entry = &self.derived_list[derived_index];
            let name = format!("{}:{}:{}", entry.sink, entry.source, entry.source_handle)
                .replace('"', "\"\"");
            write!(writer, ",\"{name}\"")?;
        }
        writeln!(writer)?;

        let total = self.timestamps.len();
        for index in start..end {
            let Some(timestamp) = self.timestamps.get(index) else {
                continue;
            };
            write!(writer, "{timestamp}")?;
            for &channel in &channels {
                let value = self.channel_value(channel, index, total);
                if value.is_finite() {
                    write!(writer, ",{value}")?;
                } else {
                    write!(writer, ",")?;
                }
            }
            for &derived_index in &derived {
                let value = self.derived_value(derived_index, index, total);
                if value.is_finite() {
                    write!(writer, ",{value}")?;
                } else {
                    write!(writer, ",")?;
                }
            }
            writeln!(writer)?;
        }
        Ok(end.saturating_sub(start))
    }
}
