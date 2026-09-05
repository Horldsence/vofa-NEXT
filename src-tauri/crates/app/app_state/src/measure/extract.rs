//! 窗口提取 — snapshot_window_budget 取数并解包为测量序列视图

use buffer_databuffer::{
    DataBuffer, DerivedSeriesSelector, WaveformSampling, WaveformSeriesSelection,
};
use parking_lot::Mutex;

use super::{ExtractedWindow, SeriesKey, WindowSeries};

/// 经 snapshot_window_budget 提取窗口 (budget 决定原始/选层)。
///
/// 请求的通道号会被 clamp 到缓冲实际通道数; 通道与派生全空返回 None。
pub(super) fn extract_window(
    buffer: &Mutex<DataBuffer>,
    window_ms: f64,
    channels: &[usize],
    derived: &[DerivedSeriesSelector],
    budget: usize,
) -> Option<ExtractedWindow> {
    let buf = buffer.lock();
    if buf.point_count() == 0 {
        return None;
    }
    let available = buf.channel_count();
    let mut selection = WaveformSeriesSelection::default();
    for &ch in channels {
        if ch < available {
            selection.channels.push(ch);
        }
    }
    selection.channels.sort_unstable();
    selection.channels.dedup();
    selection.derived = derived.to_vec();
    if selection.channels.is_empty() && selection.derived.is_empty() {
        return None;
    }
    let window = buf
        .snapshot_window_budget(-window_ms, 0.0, &selection, budget)
        .into_min_max(usize::MAX);
    drop(buf);

    let first_ts = window.timestamps.first().copied()?;
    let last_ts = window.timestamps.last().copied()?;
    let mut series = Vec::new();
    for &ch in &selection.channels {
        if let Some(values) = window.channels.get(ch).filter(|v| !v.is_empty()) {
            series.push(WindowSeries {
                key: SeriesKey::Channel(ch),
                values: values.clone(),
            });
        }
    }
    for sel in &selection.derived {
        let values = window
            .derived
            .get(&sel.sink_id)
            .and_then(|by_source| by_source.get(&sel.source_id))
            .and_then(|by_handle| by_handle.get(&sel.source_handle))
            .filter(|v| !v.is_empty());
        if let Some(values) = values {
            series.push(WindowSeries {
                key: SeriesKey::Derived {
                    sink_id: sel.sink_id.clone(),
                    source_id: sel.source_id.clone(),
                    source_handle: sel.source_handle.clone(),
                },
                values: values.clone(),
            });
        }
    }
    Some(ExtractedWindow {
        series,
        timestamps_ms: window.timestamps,
        latest_us: window.latest_timestamp_us,
        from_tier: window.sampling == WaveformSampling::MinMax,
        tier_level: window.buffer_tier.saturating_sub(1),
        span_ms: last_ts - first_ts,
    })
}

/// 相邻间隔中位数 (毫秒) — `stride` 为每组的时间戳下标步长
/// (原始 1; 层路径 2, 组内两值共享块末时间戳)。
/// 中位数抗个别缺口/块缺失, 不假设均匀分层。
pub(super) fn median_dt_ms(timestamps: &[f64], stride: usize) -> Option<f64> {
    if timestamps.len() < stride * 2 {
        return None;
    }
    let mut diffs: Vec<f64> = timestamps
        .windows(stride + 1)
        .step_by(stride)
        .map(|w| w[stride] - w[0])
        .filter(|d| d.is_finite() && *d > 0.0)
        .collect();
    if diffs.is_empty() {
        return None;
    }
    diffs.sort_by(f64::total_cmp);
    Some(diffs[diffs.len() / 2])
}
