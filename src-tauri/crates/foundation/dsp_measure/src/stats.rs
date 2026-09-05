//! 单通道窗口统计 — Vpp/极值/均值/RMS(含直流与去直流)/占空比
//!
//! 输入为金字塔快照序列: 原始路径为逐样本真值; 金字塔包络路径为 (min,max)
//! 对中点近似 — 注意 vmin/vmax/vpp 在任何层都精确 (包络极值即真值),
//! 仅 vavg/vrms 随层粗化产生近似误差。NaN 样本一律跳过。

use serde::Serialize;

/// vpp 低于该绝对阈值视为平直信号 — 周期/占空比不再有意义
pub const FLAT_SIGNAL_VPP: f64 = 1e-6;

/// 单通道窗口统计
#[derive(Debug, Clone, Serialize)]
pub struct ChannelStats {
    /// 峰峰值 (任何层都精确)
    pub vpp: f64,
    /// 最小值 (精确)
    pub vmin: f64,
    /// 最大值 (精确)
    pub vmax: f64,
    /// 均值 (包络层为中点近似)
    pub vavg: f64,
    /// RMS, 含直流分量
    pub vrms: f64,
    /// RMS, 去直流 — AC 耦合显示换算用 (sqrt(E[x²] − mean²))
    pub vrms_ac: f64,
    /// 占空比 — 中阈值 (均值) 上方时间占比 [0,1]; 平直信号为 None
    pub duty: Option<f64>,
}

/// 单遍扫描统计; 无有效 (非 NaN) 样本时返回 None
pub fn channel_stats(values: &[f32]) -> Option<ChannelStats> {
    let mut vmin = f64::INFINITY;
    let mut vmax = f64::NEG_INFINITY;
    let (mut sum, mut sq_sum) = (0.0_f64, 0.0_f64);
    let mut n = 0_usize;
    for &v in values {
        let v = f64::from(v);
        if !v.is_finite() {
            continue;
        }
        if v < vmin {
            vmin = v;
        }
        if v > vmax {
            vmax = v;
        }
        sum += v;
        sq_sum = v.mul_add(v, sq_sum);
        n += 1;
    }
    if n == 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)] // 计数转统计量, 点数上限受快照预算约束
    let n_f = n as f64;
    let vavg = sum / n_f;
    let vrms = (sq_sum / n_f).sqrt();
    let vrms_ac = vavg.mul_add(-vavg, sq_sum / n_f).max(0.0).sqrt();
    let vpp = vmax - vmin;
    let duty = if vpp > FLAT_SIGNAL_VPP {
        let threshold = vavg;
        let above = values.iter().filter(|&&v| f64::from(v) > threshold).count();
        #[allow(clippy::cast_precision_loss)]
        let duty = above as f64 / n_f;
        Some(duty)
    } else {
        None
    };
    Some(ChannelStats {
        vpp,
        vmin,
        vmax,
        vavg,
        vrms,
        vrms_ac,
        duty,
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

    #[test]
    fn sine_stats_match_closed_form() {
        // 1V 幅值正弦 @ 1kHz, 100k 采样 (~159 个整周期取整窗口)
        let n = 100_000;
        let values: Vec<f32> = (0..n)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f64 * 1e-5;
                (std::f64::consts::TAU * 1_000.0 * t).sin() as f32
            })
            .collect();
        let s = channel_stats(&values).expect("正弦有数据");
        assert!((s.vpp - 2.0).abs() < 1e-2, "vpp={}", s.vpp);
        assert!((s.vmin + 1.0).abs() < 1e-2, "vmin={}", s.vmin);
        assert!(s.vavg.abs() < 1e-2, "vavg={}", s.vavg);
        assert!(
            (s.vrms - std::f64::consts::FRAC_1_SQRT_2).abs() < 5e-3,
            "vrms={}",
            s.vrms
        );
        assert!(
            (s.vrms_ac - s.vrms).abs() < 1e-6,
            "零均值信号 vrms_ac == vrms"
        );
        assert!((s.duty.expect("正弦非平直") - 0.5).abs() < 1e-2);
    }

    #[test]
    fn dc_offset_separates_vrms_and_vrms_ac() {
        // 5V 直流 + 1V 幅值正弦: vrms ≈ 5.707, vrms_ac ≈ 0.707
        let values: Vec<f32> = (0..20_000)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f64 * 1e-4;
                (5.0 + (std::f64::consts::TAU * 50.0 * t).sin()) as f32
            })
            .collect();
        let s = channel_stats(&values).expect("有数据");
        assert!((s.vavg - 5.0).abs() < 1e-2);
        assert!((s.vrms - 5.0_f64.hypot(std::f64::consts::FRAC_1_SQRT_2)).abs() < 5e-3);
        assert!((s.vrms_ac - std::f64::consts::FRAC_1_SQRT_2).abs() < 5e-3);
    }

    #[test]
    fn square_wave_duty_reflects_asymmetry() {
        // 30% 占空比方波: 0..3 低, 3..10 高 (周期 10)
        let values: Vec<f32> = (0..10_000)
            .map(|i| if i % 10 < 3 { 0.0 } else { 1.0 })
            .collect();
        let s = channel_stats(&values).expect("有数据");
        assert!((s.vpp - 1.0).abs() < 1e-9);
        let duty = s.duty.expect("方波非平直");
        assert!((duty - 0.7).abs() < 1e-6, "duty={duty}");
    }

    #[test]
    fn flat_and_empty_signals_are_flagged() {
        assert!(channel_stats(&[]).is_none());
        assert!(channel_stats(&[f32::NAN, f32::NAN]).is_none());
        let flat = channel_stats(&[1.5_f32; 100]).expect("平直信号仍有统计");
        assert!(flat.vpp < FLAT_SIGNAL_VPP);
        assert!(flat.duty.is_none(), "平直信号占空比无意义");
        // NaN 混入不影响有效样本统计
        let mixed = channel_stats(&[1.0_f32, f32::NAN, 3.0]).expect("有有效样本");
        assert!((mixed.vmin - 1.0).abs() < 1e-9 && (mixed.vmax - 3.0).abs() < 1e-9);
    }

    #[test]
    fn micro_ripple_near_dc_is_treated_as_flat() {
        // 近直流微纹波: vpp (≈5e-7) 低于平直阈值 1e-6 → duty None (周期检测同样拒绝)
        let values: Vec<f32> = (0..4_000)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f64 * 1e-3;
                (2.5e-7 * (std::f64::consts::TAU * 10.0 * t).sin()) as f32
            })
            .collect();
        let s = channel_stats(&values).expect("有数据");
        assert!(s.vpp < FLAT_SIGNAL_VPP, "vpp={}", s.vpp);
        assert!(s.duty.is_none(), "亚阈值纹波不应给出占空比");
    }
}
