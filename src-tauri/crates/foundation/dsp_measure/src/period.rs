//! 信号周期/频率检测 — FFT 自相关为主, 迟滞过零为回退
//!
//! 主算法: 去均值 → FFT 自相关 ([`dsp_fft::normalized_autocorrelation`]) →
//! 从 `min_lag = 2` 起 (跳过 lag0 恒 1 峰) 搜索**首个** ≥ 阈值的局部极大
//! (规避 2T 次谐波陷阱) → 抛物线插值细化到亚样本。硬约束周期 ∈ [2·dt, 窗口/2]。
//!
//! 回退: 施密特触发式迟滞过零 (±10% vpp 迟滞带即去抖 — 单周期内高频纹波
//! 无法重复触发), 提议的周期还须经 ACF 同 lag 值交叉验证 (≥ 阈值),
//! 保证噪声信号两条路径都返回 None 而非误报。

use serde::Serialize;

use dsp_fft::normalized_autocorrelation;

use crate::stats::{channel_stats, FLAT_SIGNAL_VPP};

/// 自相关首峰接受阈值 (归一化 ACF)
pub const ACF_PEAK_THRESHOLD: f32 = 0.35;
/// 最小周期样本数 (硬下限, 奈奎斯特约束)
pub const MIN_LAG: usize = 2;
/// 样本数下限 — 少于该值不构成任何可测周期结构
const MIN_SAMPLES: usize = 8;

/// 周期估计结果
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PeriodEstimate {
    /// 周期 (秒)
    pub period_sec: f64,
    /// 频率 (Hz) = 1 / period
    pub freq_hz: f64,
    /// 归一化 ACF 峰值 (0..1], 越高越可信; 时域回退路径为交叉验证值
    pub confidence: f32,
}

/// 检测单通道信号的基波周期。
///
/// `values` 须为等间隔采样序列 (原始层或金字塔包络中点); 含 NaN、
/// 平直 (vpp ≤ [`FLAT_SIGNAL_VPP`]) 或非周期 (噪声/DC) 时返回 None。
pub fn detect_period(values: &[f32], dt_sec: f64) -> Option<PeriodEstimate> {
    if !(dt_sec.is_finite() && dt_sec > 0.0) || values.len() < MIN_SAMPLES {
        return None;
    }
    // 非均匀缺失会破坏相关域与时域的等间隔假设 — 直接拒绝
    if values.iter().any(|v| v.is_nan()) {
        return None;
    }
    let stats = channel_stats(values)?;
    if stats.vpp <= FLAT_SIGNAL_VPP {
        return None;
    }

    // ACF 内部去均值 — 直传原始序列即可
    let max_lag = values.len() / 2;
    let acf = normalized_autocorrelation(values, max_lag);
    if let Some(acf) = acf.as_deref() {
        if let Some(est) = peak_from_autocorrelation(acf, dt_sec) {
            return Some(est);
        }
    }
    detect_by_crossing(values, dt_sec, stats.vpp, stats.vavg, acf.as_deref())
}

/// 自相关域峰搜索: 首个 ≥ 阈值的局部极大 + 抛物线插值
fn peak_from_autocorrelation(acf: &[f32], dt_sec: f64) -> Option<PeriodEstimate> {
    #[allow(clippy::cast_precision_loss)] // 常量最小滞后 2 的浮点镜像
    let min_lag_f = MIN_LAG as f64;
    // windows(3) 中心下标 k = i+1, 从 MIN_LAG 起扫到 len-2 (k+1 恒有效)
    for (i, w) in acf.windows(3).enumerate().skip(MIN_LAG - 1) {
        let is_peak = w[1] >= ACF_PEAK_THRESHOLD && w[1] > w[0] && w[1] >= w[2];
        if !is_peak {
            continue;
        }
        let k = i + 1;
        let lag = parabolic_peak(acf, k).max(min_lag_f);
        #[allow(clippy::cast_precision_loss)] // 采样间隔到秒的量纲换算
        let period_sec = lag * dt_sec;
        let freq_hz = 1.0 / period_sec;
        return Some(PeriodEstimate {
            period_sec,
            freq_hz,
            confidence: w[1],
        });
    }
    None
}

/// 抛物线插值细化峰位 (亚样本精度); 曲率退化时返回整数峰位
fn parabolic_peak(acf: &[f32], k: usize) -> f64 {
    let (a, b, c) = (
        f64::from(acf[k - 1]),
        f64::from(acf[k]),
        f64::from(acf[k + 1]),
    );
    let denom = 2.0_f64.mul_add(-b, a) + c;
    #[allow(clippy::cast_precision_loss)]
    let k_f = k as f64;
    if denom.abs() < f64::EPSILON {
        return k_f;
    }
    let delta = (0.5 * (a - c) / denom).clamp(-0.5, 0.5);
    k_f + delta
}

/// 迟滞过零回退 — 施密特触发去抖; 提议周期须经 ACF 同 lag 交叉验证
fn detect_by_crossing(
    values: &[f32],
    dt_sec: f64,
    vpp: f64,
    mean: f64,
    acf: Option<&[f32]>,
) -> Option<PeriodEstimate> {
    let band = 0.1 * vpp;
    let (high, low) = (mean + band, mean - band);
    let mut armed = true; // 允许首个上穿触发 (首沿之前无历史可回臂)
    let mut first: Option<usize> = None;
    let mut last = 0_usize;
    let mut count = 0_usize;
    for (i, &v) in values.iter().enumerate() {
        let v = f64::from(v);
        if armed && v > high {
            if first.is_none() {
                first = Some(i);
            }
            last = i;
            count += 1;
            armed = false;
        } else if !armed && v < low {
            armed = true;
        }
    }
    let first = first?;
    if count < 2 || last <= first {
        return None;
    }
    #[allow(clippy::cast_precision_loss)] // 样本序号 × 采样间隔 → 秒
    let span_sec = (last - first) as f64 * dt_sec;
    if span_sec <= 0.0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let periods = (count - 1) as f64;
    let period_sec = span_sec / periods;
    // 交叉验证: 提议 lag 处的归一化 ACF 必须达标 (噪声在随机 lag 处 ≈ 0)
    let proposed_lag = (period_sec / dt_sec).round();
    let confidence = acf.and_then(|acf| {
        if !(proposed_lag.is_finite() && proposed_lag >= 0.0) {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 仅作 ACF 查表索引
        let idx = proposed_lag as usize;
        acf.get(idx).copied()
    });
    if confidence.is_none_or(|c| c < ACF_PEAK_THRESHOLD) {
        return None;
    }
    let freq_hz = 1.0 / period_sec;
    Some(PeriodEstimate {
        period_sec,
        freq_hz,
        confidence: confidence.unwrap_or(0.0),
    })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_lossless,
        clippy::cast_sign_loss
    )]
    use super::*;

    fn sine(freq: f64, dt: f64, n: usize, offset: f64, amp: f64) -> Vec<f32> {
        (0..n)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f64 * dt;
                amp.mul_add((std::f64::consts::TAU * freq * t).sin(), offset) as f32
            })
            .collect()
    }

    fn assert_close_to(actual: f64, expected: f64, rel: f64, what: &str) {
        let err = ((actual - expected) / expected).abs();
        assert!(
            err < rel,
            "{what}: actual={actual}, expected={expected}, err={err}"
        );
    }

    #[test]
    fn sine_period_is_accurate() {
        // 1kHz 正弦 @ 100kHz, 8192 样本 (~82 周期)
        let values = sine(1_000.0, 1e-5, 8_192, 0.0, 1.0);
        let est = detect_period(&values, 1e-5).expect("正弦应检出周期");
        assert_close_to(est.period_sec, 1e-3, 0.01, "周期");
        assert_close_to(est.freq_hz, 1_000.0, 0.01, "频率");
        assert!(est.confidence > 0.9, "纯正弦置信度应接近 1");
    }

    #[test]
    fn square_wave_period_is_accurate() {
        // 50Hz 方波 @ 10kHz, 6000 样本 (200 样本/周期, 100 高 + 100 低)
        let values: Vec<f32> = (0..6_000)
            .map(|i| if i % 200 < 100 { 1.0 } else { -1.0 })
            .collect();
        let est = detect_period(&values, 1e-4).expect("方波应检出周期");
        assert_close_to(est.period_sec, 0.02, 0.01, "方波周期");
    }

    #[test]
    fn dc_offset_does_not_change_period() {
        // 评审补充边界: 带 5V 直流偏置的正弦 — 周期不受 DC 影响
        let values = sine(100.0, 1e-4, 8_192, 5.0, 1.0);
        let est = detect_period(&values, 1e-4).expect("带偏置正弦应检出周期");
        assert_close_to(est.period_sec, 0.01, 0.01, "带偏置周期");
    }

    #[test]
    fn multi_tone_finds_fundamental_not_harmonic() {
        // 1kHz 基波 + 3kHz 三次谐波: 首峰必须落在基波 1ms (非 0.33ms / 2ms)
        let values: Vec<f32> = (0..8_192)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f64 * 1e-5;
                0.8_f64.mul_add(
                    (std::f64::consts::TAU * 3_000.0 * t).sin(),
                    (std::f64::consts::TAU * 1_000.0 * t).sin(),
                ) as f32
            })
            .collect();
        let est = detect_period(&values, 1e-5).expect("复合信号应检出周期");
        assert_close_to(est.period_sec, 1e-3, 0.05, "基波周期");
    }

    #[test]
    fn noise_returns_none() {
        // 确定性伪随机 (LCG) — 两条检测路径都必须拒绝, 不得误报频率
        let mut seed = 1_u64;
        let values: Vec<f32> = (0..4_096)
            .map(|_| {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_precision_loss,
                    clippy::cast_sign_loss
                )]
                {
                    let v = (seed >> 33) % 2_000;
                    (f64::from(u32::try_from(v).unwrap_or(1_000)) / 1_000.0 - 1.0) as f32
                }
            })
            .collect();
        assert!(detect_period(&values, 1e-4).is_none(), "噪声不应给出周期");
    }

    #[test]
    fn dc_and_micro_ripple_return_none() {
        assert!(detect_period(&[2.5_f32; 512], 1e-4).is_none(), "恒定直流");
        // 近直流微纹波 (vpp 低于平直阈值)
        let ripple: Vec<f32> = (0..512)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f64 * 1e-4;
                (2.5e-7 * (std::f64::consts::TAU * 100.0 * t).sin()) as f32
            })
            .collect();
        assert!(detect_period(&ripple, 1e-4).is_none(), "微纹波视为平直");
    }

    #[test]
    fn short_or_gapped_input_returns_none() {
        assert!(detect_period(&[1.0_f32, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0], 1e-3).is_none());
        let mut gapped = sine(1_000.0, 1e-5, 1_024, 0.0, 1.0);
        gapped[100] = f32::NAN;
        assert!(detect_period(&gapped, 1e-5).is_none(), "NaN 破坏等间隔假设");
    }
}
