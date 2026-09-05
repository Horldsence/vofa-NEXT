//! 单序列统计 — 包络极值 / 中点近似 / 守卫周期

use dsp_measure::{channel_stats, detect_period};

use super::{SeriesStats, ACF_MAX_POINTS, PERIOD_GUARD};

/// 包络极值 (跳过 NaN; 层路径极值即真值, 原始路径同样适用)
fn envelope_min(values: &[f32]) -> f64 {
    values.iter().fold(f64::INFINITY, |acc, &v| {
        if v.is_finite() {
            acc.min(f64::from(v))
        } else {
            acc
        }
    })
}

fn envelope_max(values: &[f32]) -> f64 {
    values.iter().fold(f64::NEG_INFINITY, |acc, &v| {
        if v.is_finite() {
            acc.max(f64::from(v))
        } else {
            acc
        }
    })
}

/// 周期检测 + 分辨率守卫 — 中点序列 (层) 或原始序列上执行
fn guarded_period(values: &[f32], dt_sec: f64) -> Option<dsp_measure::PeriodEstimate> {
    let est = detect_period(values, dt_sec)?;
    (est.period_sec >= PERIOD_GUARD * dt_sec).then_some(est)
}

/// 截取最近 ACF_MAX_POINTS 点 (时间升序, 等间隔不变)
fn acf_input(values: &[f32]) -> &[f32] {
    if values.len() > ACF_MAX_POINTS {
        &values[values.len() - ACF_MAX_POINTS..]
    } else {
        values
    }
}

/// 解交错 min/max 对为中点序列 (仅层路径调用; 输入长度为偶数;
/// 派生序列每对两值相同, 中点即原值)
#[allow(clippy::cast_possible_truncation)] // 中点幅值远超 f32 下溢区间
fn midpoints(values: &[f32]) -> Vec<f32> {
    values
        .chunks_exact(2)
        .map(|pair| f64::midpoint(f64::from(pair[0]), f64::from(pair[1])) as f32)
        .collect()
}

pub(super) fn measure_series(
    values: &[f32],
    is_tier: bool,
    dt_ms: Option<f64>,
) -> Option<SeriesStats> {
    if is_tier {
        let vmin = envelope_min(values);
        let vmax = envelope_max(values);
        if !vmin.is_finite() || !vmax.is_finite() {
            return None;
        }
        let mids = midpoints(values);
        let zero = dsp_measure::ChannelStats {
            vpp: 0.0,
            vmin: 0.0,
            vmax: 0.0,
            vavg: 0.0,
            vrms: 0.0,
            vrms_ac: 0.0,
            duty: None,
        };
        let stats = channel_stats(&mids).unwrap_or(zero);
        let period = dt_ms.and_then(|dt| guarded_period(acf_input(&mids), dt / 1000.0));
        Some(SeriesStats {
            vpp: vmax - vmin,
            vmin,
            vmax,
            vavg: stats.vavg,
            vrms: stats.vrms,
            vrms_ac: stats.vrms_ac,
            duty: stats.duty,
            period: period.map(|p| p.period_sec),
        })
    } else {
        let stats = channel_stats(values)?;
        let period = dt_ms.and_then(|dt| guarded_period(acf_input(values), dt / 1000.0));
        Some(SeriesStats {
            vpp: stats.vpp,
            vmin: stats.vmin,
            vmax: stats.vmax,
            vavg: stats.vavg,
            vrms: stats.vrms,
            vrms_ac: stats.vrms_ac,
            duty: stats.duty,
            period: period.map(|p| p.period_sec),
        })
    }
}
