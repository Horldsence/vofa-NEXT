//! 测量入口 — compute_source_measurements / compute_autoset_suggestion

use std::sync::Arc;

use buffer_databuffer::{DataBuffer, DerivedSeriesSelector};
use dsp_measure::{suggest_autoset, AutoSetSuggestion, ChannelFit};
use parking_lot::Mutex;

use super::{
    extract_window, measure_series, ChannelMeasurement, DerivedMeasurement, ExtractedWindow,
    SeriesKey, SourceMeasurements, AUTOSET_SEARCH_WINDOW_MS, MEASURE_BUDGET, REFINE_BUDGET,
};

/// 计算单数据源测量快照 (同步; 调用方负责放入阻塞线程)。
///
/// `channels` 为空表示全部协议通道; `derived` 为要测量的派生序列三元组。
pub fn compute_source_measurements(
    buffer: &Arc<Mutex<DataBuffer>>,
    source: &str,
    window_ms: f64,
    derived: &[DerivedSeriesSelector],
    seq: u64,
) -> Option<SourceMeasurements> {
    if !window_ms.is_finite() || window_ms <= 0.0 {
        return None;
    }
    // 全通道: 请求 0..=255 足够 (实际通道数由 extract clamp)
    let all_channels: Vec<usize> = (0..256).collect();
    let extracted = extract_window(buffer, window_ms, &all_channels, derived, MEASURE_BUDGET)?;
    let dt_ms = extracted.dt_ms();
    let is_tier = extracted.is_tier();

    let mut channels_out = Vec::new();
    let mut derived_out = Vec::new();
    for s in &extracted.series {
        let Some(stats) = measure_series(&s.values, is_tier, dt_ms) else {
            continue;
        };
        match &s.key {
            SeriesKey::Channel(channel) => channels_out.push(ChannelMeasurement {
                channel: *channel,
                vpp: stats.vpp,
                vmin: stats.vmin,
                vmax: stats.vmax,
                vavg: stats.vavg,
                vrms: stats.vrms,
                vrms_ac: stats.vrms_ac,
                duty: stats.duty,
                freq: stats.period.map(|p| 1.0 / p),
                period: stats.period,
            }),
            SeriesKey::Derived {
                sink_id,
                source_id,
                source_handle,
            } => derived_out.push(DerivedMeasurement {
                sink_id: sink_id.clone(),
                source_id: source_id.clone(),
                source_handle: source_handle.clone(),
                vpp: stats.vpp,
                vmin: stats.vmin,
                vmax: stats.vmax,
                vavg: stats.vavg,
                vrms: stats.vrms,
                vrms_ac: stats.vrms_ac,
                duty: stats.duty,
                freq: stats.period.map(|p| 1.0 / p),
                period: stats.period,
            }),
        }
    }
    if channels_out.is_empty() && derived_out.is_empty() {
        return None;
    }
    Some(SourceMeasurements {
        source: source.to_string(),
        seq,
        window_ms,
        latest_timestamp_us: extracted.latest_us,
        from_tier: extracted.from_tier,
        tier_level: extracted.tier_level,
        channels: channels_out,
        derived: derived_out,
    })
}

/// 计算自动设置建议 (同步; 调用方负责放入阻塞线程)。
///
/// 搜索最近 [`AUTOSET_SEARCH_WINDOW_MS`] 窗口: 周期可测 → 显示
/// [`dsp_measure::PERIODS_SHOWN`] 个**最慢基波周期** (协议通道与派生序列
/// 一起参与取最大); 全部不可测 → 回退快照实际数据跨度拟合。
/// 守卫不满足 (粗层测不到快周期) 时细化重查一次 (预算 ×4)。
pub fn compute_autoset_suggestion(
    buffer: &Arc<Mutex<DataBuffer>>,
    channels: &[usize],
    derived: &[DerivedSeriesSelector],
    shared_y: bool,
    current_v_per_div: &[f64],
) -> Option<AutoSetSuggestion> {
    let requested: Vec<usize> = if channels.is_empty() {
        (0..256).collect()
    } else {
        channels.to_vec()
    };
    let extracted = extract_window(
        buffer,
        AUTOSET_SEARCH_WINDOW_MS,
        &requested,
        derived,
        MEASURE_BUDGET,
    )?;
    let mut fits = build_fits(&extracted);
    // 细化重查: 有序列处于层路径且周期被守卫拒绝 → 预算 ×4 越一层重测
    if extracted.is_tier() && fits.iter().any(|fit| fit.period_sec.is_none()) {
        if let Some(refined) = extract_window(
            buffer,
            AUTOSET_SEARCH_WINDOW_MS,
            &requested,
            derived,
            REFINE_BUDGET,
        ) {
            let refined_fits = build_fits(&refined);
            for (fit, better) in fits.iter_mut().zip(refined_fits) {
                if fit.period_sec.is_none() {
                    fit.period_sec = better.period_sec;
                }
            }
        }
    }
    if fits.is_empty() {
        return None;
    }
    // 平直信号保持现值: 通道按请求下标取, 派生序列无现值传 0 (触发默认 1)
    let mut current = Vec::with_capacity(fits.len());
    let mut derived_seen = 0_usize;
    for s in &extracted.series {
        match &s.key {
            SeriesKey::Channel(ch) => {
                current.push(current_v_per_div.get(*ch).copied().unwrap_or(1.0));
            }
            SeriesKey::Derived { .. } => {
                derived_seen += 1;
                current.push(0.0);
            }
        }
    }
    let _ = derived_seen;
    let fallback_window_sec = (extracted.span_ms / 1000.0).max(0.0);
    Some(suggest_autoset(
        &fits,
        shared_y,
        &current,
        fallback_window_sec,
    ))
}

/// 从提取窗口组装每序列拟合输入 (包络极值 + 守卫周期), 顺序与 series 一致
fn build_fits(extracted: &ExtractedWindow) -> Vec<ChannelFit> {
    let dt_ms = extracted.dt_ms();
    let is_tier = extracted.is_tier();
    extracted
        .series
        .iter()
        .filter_map(|s| {
            let stats = measure_series(&s.values, is_tier, dt_ms)?;
            Some(ChannelFit {
                vmin: stats.vmin,
                vmax: stats.vmax,
                vpp: stats.vpp,
                period_sec: stats.period,
            })
        })
        .collect()
}
