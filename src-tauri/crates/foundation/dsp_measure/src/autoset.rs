//! 示波器自动设置建议 — 时基按周期取档 + 每通道 V/div 1-2-5 拟合
//!
//! 唯一实现原则: 1-2-5 向上取档与时基档位表在本模块定义; 前端
//! `TIME_BASES_SEC` / `V_PER_DIV` 仅为 UI 旋钮常量 (数值镜像, 注释互指)。
//!
//! 时基策略: 显示 [`PERIODS_SHOWN`] 个最慢基波周期的波形 (2 个周期足够观察;
//! 时基只要求 1-2-5 档位精度, 包络层中点序列的自相关远超所需)。周期不可测时
//! 回退为「最近可用数据窗口」拟合 (历史行为)。时基恒向上取档 — 显示的周期数
//! 只多不少; 仅当目标超出档位表上限才置 `clamped` (未完整显示目标周期数)。

use serde::Serialize;

use crate::stats::FLAT_SIGNAL_VPP;

/// 水平格数 (与前端 10 div 一致)
pub const H_DIVS: f64 = 10.0;
/// 垂直格数 (8 div)
pub const V_DIVS: f64 = 8.0;
/// 信号垂直目标占比 (70%, 上下各留 ~15% 余量, 避免顶满)
pub const VERTICAL_FILL_RATIO: f64 = 0.7;
/// 时基自动设置显示的周期数 — 按最慢基波周期计 (2 个周期足够观察)
pub const PERIODS_SHOWN: f64 = 2.0;

/// 1-2-5 序列时基档位表 (秒/格) — 与前端 `TIME_BASES_SEC` 数值镜像
pub const TIME_BASES_SEC: [f64; 15] = [
    100e-6, 200e-6, 500e-6, //
    1e-3, 2e-3, 5e-3, //
    10e-3, 20e-3, 50e-3, //
    100e-3, 200e-3, 500e-3, //
    1.0, 2.0, 5.0,
];

/// 单通道拟合输入 (由调用方从 [`crate::channel_stats`] + [`crate::detect_period`] 组装)
#[derive(Debug, Clone)]
pub struct ChannelFit {
    pub vmin: f64,
    pub vmax: f64,
    pub vpp: f64,
    /// 已通过分辨率守卫的基波周期; 不可测为 None
    pub period_sec: Option<f64>,
}

/// 单通道自动设置结果
#[derive(Debug, Clone, Serialize)]
pub struct AutoSetChannel {
    /// V/div (1-2-5 档; 平直信号保持现值)
    pub v_per_div: f64,
    /// 垂直偏移 (信号中点居中)
    pub position: f64,
    /// 该通道检出的基波周期 (透传展示)
    pub period_sec: Option<f64>,
}

/// AutoSet 建议 — 前端直接合并进 ScopeAxisConfig (状态仍归前端持有)
#[derive(Debug, Clone, Serialize)]
pub struct AutoSetSuggestion {
    /// 时基 (秒/格), 已取 1-2-5 档
    pub time_base_sec: f64,
    /// 实际显示窗口 = time_base × H_DIVS
    pub window_sec: f64,
    /// 周期推导的原始需求窗口 (钳位诊断用)
    pub requested_window_sec: f64,
    /// 时基被钳到档位表上限 — 未完整显示目标周期数
    pub clamped: bool,
    /// sharedY 下通道幅值/偏置差异过大 — 小信号通道可能被压扁
    pub shared_y_span_risk: bool,
    pub channels: Vec<AutoSetChannel>,
    pub h_position: f64,
    pub running: bool,
}

/// 1-2-5 向上取档 (任意数量级, 不限于档位表 — 小信号可取到表外 µV/nV 档)
pub fn snap_up_1_2_5(target: f64) -> f64 {
    if !target.is_finite() || target <= 0.0 {
        return 1.0;
    }
    #[allow(clippy::cast_precision_loss)] // 数量级换算, f64 指数精度足够
    let decade = 10_f64.powf(target.log10().floor());
    let mantissa_target = target / decade;
    // 容差抵消浮点误差 (如 0.3/0.1 = 2.9999999999999996)
    let mantissa = if mantissa_target <= 1.0 + 1e-9 {
        1.0
    } else if mantissa_target <= 2.0 + 1e-9 {
        2.0
    } else if mantissa_target <= 5.0 + 1e-9 {
        5.0
    } else {
        10.0
    };
    mantissa * decade
}

/// 由各通道拟合结果生成自动设置建议。
///
/// - `fallback_window_sec`: 无任何周期可测时的回退窗口 (通常为快照实际数据跨度);
/// - `current_v_per_div`: 平直信号通道保持现值 (按通道下标取, 缺省 1)。
pub fn suggest_autoset(
    fits: &[ChannelFit],
    shared_y: bool,
    current_v_per_div: &[f64],
    fallback_window_sec: f64,
) -> AutoSetSuggestion {
    let max_period = fits
        .iter()
        .filter_map(|fit| fit.period_sec)
        .fold(0.0_f64, f64::max);
    let requested_window_sec = if max_period > 0.0 {
        PERIODS_SHOWN * max_period
    } else {
        fallback_window_sec.max(0.0)
    };
    let target_tb = requested_window_sec / H_DIVS;
    // 容差取档: FFT 测得的周期带浮点尾差, 恰落在表项边界上的目标
    // (如 200µs+1e-8) 不应跳到下一档 — 相对容差远小于 1-2-5 档距
    let (time_base_sec, clamped) = TIME_BASES_SEC
        .iter()
        .copied()
        .find(|tb| *tb >= target_tb * (1.0 - 1e-6))
        .map_or_else(
            || (TIME_BASES_SEC[TIME_BASES_SEC.len() - 1], true),
            |tb| (tb, false),
        );
    let window_sec = time_base_sec * H_DIVS;

    let keep_current = |idx: usize| -> f64 {
        current_v_per_div
            .get(idx)
            .copied()
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(1.0)
    };

    let mut channels = Vec::with_capacity(fits.len());
    let mut shared_y_span_risk = false;
    if shared_y {
        // 共用 Y: v_per_div 取 max(各通道 Vpp) — 避免全局 min/max 跨度被
        // 大幅值通道撑爆后压扁小信号; position 仍取全局中点
        let max_vpp = fits.iter().map(|fit| fit.vpp).fold(0.0_f64, f64::max);
        let v_per_div = if max_vpp > FLAT_SIGNAL_VPP {
            snap_up_1_2_5(max_vpp / (V_DIVS * VERTICAL_FILL_RATIO))
        } else {
            keep_current(0)
        };
        let span_min = fits
            .iter()
            .map(|fit| fit.vmin)
            .fold(f64::INFINITY, f64::min);
        let span_max = fits
            .iter()
            .map(|fit| fit.vmax)
            .fold(f64::NEG_INFINITY, f64::max);
        let position = if span_min.is_finite() && span_max.is_finite() {
            shared_y_span_risk = (span_max - span_min) > V_DIVS * VERTICAL_FILL_RATIO * v_per_div;
            f64::midpoint(span_max, span_min)
        } else {
            0.0
        };
        for fit in fits {
            channels.push(AutoSetChannel {
                v_per_div,
                position,
                period_sec: fit.period_sec,
            });
        }
    } else {
        for (idx, fit) in fits.iter().enumerate() {
            let v_per_div = if fit.vpp > FLAT_SIGNAL_VPP {
                snap_up_1_2_5(fit.vpp / (V_DIVS * VERTICAL_FILL_RATIO))
            } else {
                keep_current(idx)
            };
            channels.push(AutoSetChannel {
                v_per_div,
                position: f64::midpoint(fit.vmax, fit.vmin),
                period_sec: fit.period_sec,
            });
        }
    }

    AutoSetSuggestion {
        time_base_sec,
        window_sec,
        requested_window_sec,
        clamped,
        shared_y_span_risk,
        channels,
        h_position: 0.0,
        running: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(actual: f64, expected: f64, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "{what}: actual={actual}, expected={expected}"
        );
    }

    #[test]
    fn snap_up_1_2_5_rounds_to_next_step() {
        approx(snap_up_1_2_5(0.4), 0.5, "0.4→0.5");
        approx(snap_up_1_2_5(0.2), 0.2, "档内不动");
        approx(snap_up_1_2_5(0.201), 0.5, "0.201→0.5");
        approx(snap_up_1_2_5(0.023), 0.05, "0.023→0.05");
        approx(snap_up_1_2_5(7_000.0), 10_000.0, "7000→10k");
        approx(snap_up_1_2_5(2.5e-7), 5e-7, "小信号表外档");
        approx(snap_up_1_2_5(0.0), 1.0, "非法输入回退 1");
        approx(snap_up_1_2_5(f64::NAN), 1.0, "NaN 回退 1");
    }

    #[test]
    fn one_hz_sine_shows_two_periods() {
        // 1Hz 正弦 (Vpp 2V): 窗口 = 2×1s = 2s → 时基 0.2s/div (表内直取)
        let fits = vec![ChannelFit {
            vmin: -1.0,
            vmax: 1.0,
            vpp: 2.0,
            period_sec: Some(1.0),
        }];
        let s = suggest_autoset(&fits, false, &[1.0], 10.0);
        assert!(!s.clamped);
        approx(s.time_base_sec, 0.2, "时基 0.2s/div");
        approx(s.window_sec, 2.0, "窗口 2s");
        approx(s.requested_window_sec, 2.0, "需求窗口 2s");
        // V/div: 2 / (8×0.7) = 0.357 → 0.5
        approx(s.channels[0].v_per_div, 0.5, "V/div 0.5");
        approx(s.channels[0].position, 0.0, "居中");
        assert!(!s.shared_y_span_risk);
        approx(s.h_position, 0.0, "hPosition 归零");
        assert!(s.running);
    }

    #[test]
    fn slow_period_beyond_table_is_clamped() {
        // 200s 周期 → 需求窗口 400s → 目标时基 40s/div > 5s/div 上限
        let fits = vec![ChannelFit {
            vmin: -1.0,
            vmax: 1.0,
            vpp: 2.0,
            period_sec: Some(200.0),
        }];
        let s = suggest_autoset(&fits, false, &[1.0], 10.0);
        assert!(s.clamped, "超出档位表应置钳位标志");
        approx(s.time_base_sec, 5.0, "钳到最大档");
        approx(s.requested_window_sec, 400.0, "需求窗口保留");
        approx(s.window_sec, 50.0, "实际窗口 50s");
    }

    #[test]
    fn unmeasurable_period_falls_back_to_data_span() {
        let fits = vec![ChannelFit {
            vmin: 0.0,
            vmax: 3.3,
            vpp: 3.3,
            period_sec: None,
        }];
        let s = suggest_autoset(&fits, false, &[1.0], 8.0);
        assert!(!s.clamped);
        approx(s.requested_window_sec, 8.0, "回退到数据跨度");
        approx(s.time_base_sec, 1.0, "0.8 → 向上取 1s/div");
    }

    #[test]
    fn flat_channel_keeps_current_v_per_div() {
        let fits = vec![ChannelFit {
            vmin: 2.4,
            vmax: 2.4,
            vpp: 0.0,
            period_sec: None,
        }];
        let s = suggest_autoset(&fits, false, &[0.05], 8.0);
        approx(s.channels[0].v_per_div, 0.05, "平直信号保持现值");
        approx(s.channels[0].position, 2.4, "居中到信号值");
    }

    #[test]
    fn shared_y_uses_max_vpp_not_global_span() {
        // 大信号 5V + 小信号 50mV (同偏置): v_per_div 由 5V 决定, 小信号不被全局跨度压扁
        let fits = vec![
            ChannelFit {
                vmin: 0.0,
                vmax: 5.0,
                vpp: 5.0,
                period_sec: Some(1.0),
            },
            ChannelFit {
                vmin: 0.0,
                vmax: 0.05,
                vpp: 0.05,
                period_sec: Some(1.0),
            },
        ];
        let s = suggest_autoset(&fits, true, &[1.0, 1.0], 10.0);
        approx(s.channels[0].v_per_div, 1.0, "max Vpp → 1V/div");
        approx(s.channels[1].v_per_div, 1.0, "共用同一档");
        assert!(!s.shared_y_span_risk, "同偏置时无压扁风险");
        approx(s.channels[0].position, 2.5, "全局中点");

        // 偏置差异大: 小信号通道在远处 → 提示风险
        let offset = vec![
            ChannelFit {
                vmin: 0.0,
                vmax: 5.0,
                vpp: 5.0,
                period_sec: Some(1.0),
            },
            ChannelFit {
                vmin: 100.0,
                vmax: 100.05,
                vpp: 0.05,
                period_sec: Some(1.0),
            },
        ];
        let s2 = suggest_autoset(&offset, true, &[1.0, 1.0], 10.0);
        assert!(s2.shared_y_span_risk, "跨度和远超 V/div 应提示风险");
    }

    #[test]
    fn slowest_channel_drives_time_base() {
        // 0.5Hz + 2Hz 混合: 由最慢 2s 周期决定 → 窗口 4s → 时基 0.5s/div
        let fits = vec![
            ChannelFit {
                vmin: -1.0,
                vmax: 1.0,
                vpp: 2.0,
                period_sec: Some(2.0),
            },
            ChannelFit {
                vmin: -1.0,
                vmax: 1.0,
                vpp: 2.0,
                period_sec: Some(0.5),
            },
        ];
        let s = suggest_autoset(&fits, false, &[1.0, 1.0], 10.0);
        approx(s.time_base_sec, 0.5, "0.4 → 0.5s/div");
    }
}
