//! DataBuffer 窗口查询 — 区间下标扫描 / 原始窗口构建 / CSV 导出

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

use crate::DataBuffer;

use super::{DerivedSeriesSelector, WaveformSampling, WaveformSeriesSelection, WaveformWindow};

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

    // 微秒时间戳 (~1.6e15) 转 f64 毫秒仅损失亚微秒精度, 显示用途足够
    #[allow(clippy::cast_precision_loss)]
    pub(super) fn relative_timestamp_ms(timestamp_us: u64, latest_us: u64) -> f64 {
        if timestamp_us <= latest_us {
            -((latest_us - timestamp_us) as f64) / 1000.0
        } else {
            ((timestamp_us - latest_us) as f64) / 1000.0
        }
    }

    /// 兼容查询：获取相对时间窗口内的全部原始点。
    pub fn get_window(&self, start_ms: i64, end_ms: i64) -> WaveformWindow {
        // 毫秒量级 i64 -> f64 无精度损失 (2^53 以内整数可精确表示)
        #[allow(clippy::cast_precision_loss)]
        {
            self.get_window_raw(start_ms as f64, end_ms as f64)
        }
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
    pub(super) fn timestamp_at_offset(latest_us: u64, offset_ms: f64) -> u64 {
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

    pub(super) fn range_indices(&self, start_ms: f64, end_ms: f64) -> (usize, usize, u64) {
        let total = self.timestamps.len();
        let latest_us = self
            .timestamps
            .get(total.saturating_sub(1))
            .copied()
            .unwrap_or(0);
        if total == 0 {
            return (0, 0, latest_us);
        }
        let from_us = Self::timestamp_at_offset(latest_us, start_ms.min(end_ms));
        let to_us = Self::timestamp_at_offset(latest_us, start_ms.max(end_ms));
        (
            self.lower_bound_timestamp(from_us),
            self.upper_bound_timestamp(to_us),
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

    pub(crate) fn channel_value(&self, channel: usize, index: usize, total: usize) -> f32 {
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

    fn selected_derived_indices(&self, selection: &WaveformSeriesSelection) -> Vec<usize> {
        self.derived
            .lock()
            .entries()
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

    pub(super) fn all_series_selection(&self) -> WaveformSeriesSelection {
        WaveformSeriesSelection {
            channels: (0..self.num_channels).collect(),
            derived: self
                .derived
                .lock()
                .entries()
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
        // 窗口绝对时间戳 (升序) — 派生序列按时间精确对齐到该轴
        let window_ts: Vec<u64> = selected
            .iter()
            .filter_map(|&index| self.timestamps.get(index).copied())
            .collect();
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
        {
            let store = self.derived.lock();
            for (derived_index, entry) in store.entries().iter().enumerate() {
                if entry.rb.is_empty() || !selected_derived.contains(&derived_index) {
                    continue;
                }
                let values = store.values_at_timestamps(derived_index, &window_ts);
                derived
                    .entry(entry.sink.clone())
                    .or_default()
                    .entry(entry.source.clone())
                    .or_default()
                    .insert(entry.source_handle.clone(), values);
            }
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
            storage_overflow: self.storage_overflow,
            buffer_tier: 0,
        }
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

        // 行按 index 升序 = 时间戳升序; 派生值按时间对齐预取 (缺口 → 空单元格)
        let row_ts: Vec<u64> = (start..end)
            .filter_map(|index| self.timestamps.get(index).copied())
            .collect();
        let derived_columns: Vec<(String, Vec<f32>)> = {
            let store = self.derived.lock();
            derived
                .iter()
                .filter_map(|&derived_index| {
                    let entry = store.entry(derived_index)?;
                    let name = format!("{}:{}:{}", entry.sink, entry.source, entry.source_handle)
                        .replace('"', "\"\"");
                    Some((name, store.values_at_timestamps(derived_index, &row_ts)))
                })
                .collect()
        };

        write!(writer, "timestamp_us")?;
        for channel in &channels {
            write!(writer, ",CH{channel}")?;
        }
        for (name, _) in &derived_columns {
            write!(writer, ",\"{name}\"")?;
        }
        writeln!(writer)?;

        let total = self.timestamps.len();
        for (row, index) in (start..end).enumerate() {
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
            for (_, values) in &derived_columns {
                let value = values.get(row).copied().unwrap_or(f32::NAN);
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
