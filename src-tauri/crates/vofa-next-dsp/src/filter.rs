//! 数字滤波器 — FIR (有限脉冲响应) 与 IIR biquad (二阶无限脉冲响应)
//!
//! 含预设模板: 低通/高通/带通/带阻 (Butterworth-style biquad)
//!
//! 用法:
//! ```
//! use vofa_next_dsp::{DigitalFilter, FilterPreset};
//! let mut f = DigitalFilter::from_preset(FilterPreset::Lowpass { cutoff: 100.0, sample_rate: 1000.0 });
//! let out = f.process(0.5);
//! ```

use serde::{Deserialize, Serialize};

/// 滤波器类型 — FIR (任意阶) 或 IIR biquad (二阶)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterKind {
    /// FIR 滤波器 — 前馈, 无反馈, 稳定
    /// b: 分子系数 (前馈), 长度 = 阶数 + 1
    /// 输出 y[n] = sum(b[k] * x[n-k], k=0..N)
    FIR { b: Vec<f32> },
    /// IIR biquad (二阶) — 标准形式
    /// b: 分子系数 [b0, b1, b2]
    /// a: 分母系数 [a0, a1, a2] (a0 通常为 1.0)
    /// 输出 y[n] = (b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]) / a0
    IIR { b: [f32; 3], a: [f32; 3] },
}

/// 滤波器预设 — 提供常用模板 (用户也可自定义 FilterKind)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FilterPreset {
    /// 低通 — 截止频率以下的信号通过
    Lowpass { cutoff: f32, sample_rate: f32 },
    /// 高通 — 截止频率以上的信号通过
    Highpass { cutoff: f32, sample_rate: f32 },
    /// 带通 — [low, high] 频率范围内的信号通过
    Bandpass {
        low: f32,
        high: f32,
        sample_rate: f32,
    },
    /// 带阻 (陷波) — [low, high] 频率范围内的信号衰减
    Bandstop {
        low: f32,
        sample_rate: f32,
        high: f32,
    },
}

/// 数字滤波器 — 维护内部状态, 逐点处理
pub struct DigitalFilter {
    kind: FilterKind,
    /// FIR 状态: 输入延迟线 x[n-1], x[n-2], ... (长度 = b.len() - 1)
    fir_state: Vec<f32>,
    /// IIR 状态: [x[n-1], x[n-2], y[n-1], y[n-2]]
    iir_state: [f32; 4],
}

impl DigitalFilter {
    /// 从 FilterKind 创建
    pub fn new(kind: FilterKind) -> Self {
        let fir_len = match &kind {
            FilterKind::FIR { b } => b.len().saturating_sub(1),
            FilterKind::IIR { .. } => 0,
        };
        Self {
            kind,
            fir_state: vec![0.0; fir_len],
            iir_state: [0.0; 4],
        }
    }

    /// 从预设创建 (biquad 实现)
    pub fn from_preset(preset: FilterPreset) -> Self {
        let (b, a) = match preset {
            FilterPreset::Lowpass {
                cutoff,
                sample_rate,
            } => lowpass_biquad(cutoff, sample_rate),
            FilterPreset::Highpass {
                cutoff,
                sample_rate,
            } => highpass_biquad(cutoff, sample_rate),
            FilterPreset::Bandpass {
                low,
                high,
                sample_rate,
            } => bandpass_biquad(low, high, sample_rate),
            FilterPreset::Bandstop {
                low,
                high,
                sample_rate,
            } => bandstop_biquad(low, high, sample_rate),
        };
        Self::new(FilterKind::IIR { b, a })
    }

    /// 逐点处理
    #[allow(clippy::suboptimal_flops)]
    pub fn process(&mut self, input: f32) -> f32 {
        match &self.kind {
            FilterKind::FIR { b } => {
                // y[n] = b[0]*x[n] + b[1]*x[n-1] + ... + b[N]*x[n-N]
                let mut y = b[0] * input;
                for (i, &bi) in b.iter().enumerate().skip(1) {
                    let s = self.fir_state.get(i - 1).copied().unwrap_or(0.0);
                    y += bi * s;
                }
                // 更新延迟线 (新输入 push 到 front, 旧的下移)
                if !self.fir_state.is_empty() {
                    let len = self.fir_state.len();
                    for i in (1..len).rev() {
                        self.fir_state[i] = self.fir_state[i - 1];
                    }
                    self.fir_state[0] = input;
                }
                y
            }
            FilterKind::IIR { b, a } => {
                let x1 = self.iir_state[0];
                let x2 = self.iir_state[1];
                let y1 = self.iir_state[2];
                let y2 = self.iir_state[3];
                let a0 = a[0];
                let y = (b[0] * input + b[1] * x1 + b[2] * x2 - a[1] * y1 - a[2] * y2) / a0;
                // 更新状态
                self.iir_state[1] = x1;
                self.iir_state[0] = input;
                self.iir_state[3] = y1;
                self.iir_state[2] = y;
                y
            }
        }
    }

    /// 重置状态 (清空延迟线)
    pub fn reset(&mut self) {
        self.fir_state.fill(0.0);
        self.iir_state = [0.0; 4];
    }

    /// 获取滤波器类型
    pub const fn kind(&self) -> &FilterKind {
        &self.kind
    }
}

// ============ Biquad 系数计算 (RBJ Audio EQ Cookbook) ============
//
// 参考: https://www.musicdsp.org/en/latest/Filters/197-rbj-audio-eq-cookbook.html
//
// w0 = 2 * pi * fc / fs  (归一化角频率)
// alpha = sin(w0) / (2 * Q)  (Q 默认 1/sqrt(2) ≈ 0.707, Butterworth 响应)
//
// 低通: b0 = (1 - cos w0) / 2, b1 = 1 - cos w0, b2 = (1 - cos w0) / 2
//       a0 = 1 + alpha, a1 = -2 cos w0, a2 = 1 - alpha
// 高通: b0 = (1 + cos w0) / 2, b1 = -(1 + cos w0), b2 = (1 + cos w0) / 2
//       a0 = 1 + alpha, a1 = -2 cos w0, a2 = 1 - alpha
// 带通 (常量 0 dB 峰值): b0 = alpha, b1 = 0, b2 = -alpha
//       a0 = 1 + alpha, a1 = -2 cos w0, a2 = 1 - alpha
// 带阻 (陷波): b0 = 1, b1 = -2 cos w0, b2 = 1
//       a0 = 1 + alpha, a1 = -2 cos w0, a2 = 1 - alpha

const PI_F32: f32 = std::f32::consts::PI;

/// Q 因子 (默认 Butterworth, 1/√2)
const DEFAULT_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// 计算归一化角频率 w0 = 2*pi*fc/fs
fn w0(cutoff: f32, sample_rate: f32) -> f32 {
    2.0 * PI_F32 * cutoff / sample_rate
}

/// alpha = sin(w0) / (2 * Q)
fn alpha(w0: f32, q: f32) -> f32 {
    w0.sin() / (2.0 * q)
}

/// 低通 biquad 系数 (fc=截止频率, fs=采样率)
pub fn lowpass_biquad(cutoff: f32, sample_rate: f32) -> ([f32; 3], [f32; 3]) {
    let w = w0(cutoff, sample_rate);
    let a = alpha(w, DEFAULT_Q);
    let cos_w = w.cos();
    let b0 = (1.0 - cos_w) / 2.0;
    let b1 = 1.0 - cos_w;
    let b2 = (1.0 - cos_w) / 2.0;
    let a0 = 1.0 + a;
    let a1 = -2.0 * cos_w;
    let a2 = 1.0 - a;
    ([b0, b1, b2], [a0, a1, a2])
}

/// 高通 biquad 系数
pub fn highpass_biquad(cutoff: f32, sample_rate: f32) -> ([f32; 3], [f32; 3]) {
    let w = w0(cutoff, sample_rate);
    let a = alpha(w, DEFAULT_Q);
    let cos_w = w.cos();
    let b0 = f32::midpoint(1.0, cos_w);
    let b1 = -(1.0 + cos_w);
    let b2 = f32::midpoint(1.0, cos_w);
    let a0 = 1.0 + a;
    let a1 = -2.0 * cos_w;
    let a2 = 1.0 - a;
    ([b0, b1, b2], [a0, a1, a2])
}

/// 带通 biquad 系数 (常量 0 dB 峰值)
/// low, high: 通带 [low, high]
/// 中心频率 fc = sqrt(low * high), 带宽 BW = high - low
pub fn bandpass_biquad(low: f32, high: f32, sample_rate: f32) -> ([f32; 3], [f32; 3]) {
    let fc = (low * high).sqrt();
    let bw = high - low;
    let w = w0(fc, sample_rate);
    // 对于带通: Q = fc / BW
    let q = if bw > 0.0 { fc / bw } else { DEFAULT_Q };
    let a = alpha(w, q);
    let cos_w = w.cos();
    let b0 = a;
    let b1 = 0.0;
    let b2 = -a;
    let a0 = 1.0 + a;
    let a1 = -2.0 * cos_w;
    let a2 = 1.0 - a;
    ([b0, b1, b2], [a0, a1, a2])
}

/// 带阻 (陷波) biquad 系数
pub fn bandstop_biquad(low: f32, high: f32, sample_rate: f32) -> ([f32; 3], [f32; 3]) {
    let fc = (low * high).sqrt();
    let bw = high - low;
    let w = w0(fc, sample_rate);
    let q = if bw > 0.0 { fc / bw } else { DEFAULT_Q };
    let a = alpha(w, q);
    let cos_w = w.cos();
    let b0 = 1.0;
    let b1 = -2.0 * cos_w;
    let b2 = 1.0;
    let a0 = 1.0 + a;
    let a1 = -2.0 * cos_w;
    let a2 = 1.0 - a;
    ([b0, b1, b2], [a0, a1, a2])
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn test_fir_passthrough() {
        // b = [1.0] → FIR 通过 (y = x)
        let mut f = DigitalFilter::new(FilterKind::FIR { b: vec![1.0] });
        assert!((f.process(0.5) - 0.5).abs() < 1e-6);
        assert!((f.process(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_fir_delay() {
        // b = [0.0, 1.0] → 延迟一拍 (y[n] = x[n-1])
        let mut f = DigitalFilter::new(FilterKind::FIR { b: vec![0.0, 1.0] });
        assert!((f.process(1.0) - 0.0).abs() < 1e-6); // 第一次: x[-1]=0
        assert!((f.process(2.0) - 1.0).abs() < 1e-6); // 第二次: x[0]=1
        assert!((f.process(3.0) - 2.0).abs() < 1e-6); // 第三次: x[1]=2
    }

    #[test]
    fn test_fir_moving_average() {
        // b = [0.5, 0.5] → 移动平均
        let mut f = DigitalFilter::new(FilterKind::FIR { b: vec![0.5, 0.5] });
        assert!((f.process(2.0) - 1.0).abs() < 1e-6); // (2+0)/2
        assert!((f.process(4.0) - 3.0).abs() < 1e-6); // (4+2)/2
        assert!((f.process(6.0) - 5.0).abs() < 1e-6); // (6+4)/2
    }

    #[test]
    fn test_iir_passthrough() {
        // b = [1, 0, 0], a = [1, 0, 0] → y = x
        let mut f = DigitalFilter::new(FilterKind::IIR {
            b: [1.0, 0.0, 0.0],
            a: [1.0, 0.0, 0.0],
        });
        assert!((f.process(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_lowpass_attenuates_high_freq() {
        // 采样率 1000 Hz, 截止 100 Hz
        // 输入 50 Hz (低频) 应通过, 400 Hz (高频) 应衰减
        let fs = 1000.0;
        let mut f_lp = DigitalFilter::from_preset(FilterPreset::Lowpass {
            cutoff: 100.0,
            sample_rate: fs,
        });
        let mut f_lp_high = DigitalFilter::from_preset(FilterPreset::Lowpass {
            cutoff: 100.0,
            sample_rate: fs,
        });

        // 测试 50 Hz (低频)
        let n = 200;
        let low_freq = 50.0;
        let mut max_lo = 0.0f32;
        for i in 0..n {
            let x = (2.0 * PI * low_freq * i as f32 / fs).sin();
            let y = f_lp.process(x);
            if i > 50 {
                // 稳态后测量幅值
                max_lo = max_lo.max(y.abs());
            }
        }

        // 测试 400 Hz (高频)
        let high_freq = 400.0;
        let mut max_hi = 0.0f32;
        for i in 0..n {
            let x = (2.0 * PI * high_freq * i as f32 / fs).sin();
            let y = f_lp_high.process(x);
            if i > 50 {
                max_hi = max_hi.max(y.abs());
            }
        }

        // 低频幅值应显著大于高频幅值 (衰减 > 50%)
        assert!(
            max_lo > max_hi * 2.0,
            "低频 {max_lo} 应明显大于高频 {max_hi} 的 2 倍"
        );
    }

    #[test]
    fn test_highpass_attenuates_low_freq() {
        let fs = 1000.0;
        let mut f_hp = DigitalFilter::from_preset(FilterPreset::Highpass {
            cutoff: 200.0,
            sample_rate: fs,
        });
        let mut f_hp_high = DigitalFilter::from_preset(FilterPreset::Highpass {
            cutoff: 200.0,
            sample_rate: fs,
        });

        // 低频 50 Hz (应衰减)
        let n = 200;
        let mut max_lo = 0.0f32;
        for i in 0..n {
            let x = (2.0 * PI * 50.0 * i as f32 / fs).sin();
            let y = f_hp.process(x);
            if i > 50 {
                max_lo = max_lo.max(y.abs());
            }
        }

        // 高频 400 Hz (应通过)
        let mut max_hi = 0.0f32;
        for i in 0..n {
            let x = (2.0 * PI * 400.0 * i as f32 / fs).sin();
            let y = f_hp_high.process(x);
            if i > 50 {
                max_hi = max_hi.max(y.abs());
            }
        }

        assert!(
            max_hi > max_lo * 2.0,
            "高频 {max_hi} 应明显大于低频 {max_lo} 的 2 倍"
        );
    }

    #[test]
    fn test_iir_stability() {
        // biquad 系数 a0 应为正 (稳定的 biquad)
        let (b, a) = lowpass_biquad(100.0, 1000.0);
        assert!(a[0] > 0.0);
        assert!(b.iter().all(|v| v.is_finite()));
        assert!(a.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn test_reset() {
        let mut f = DigitalFilter::new(FilterKind::FIR { b: vec![0.5, 0.5] });
        f.process(1.0);
        f.process(2.0);
        f.reset();
        // 重置后输出应等同于首次处理
        let y = f.process(3.0);
        assert!((y - 1.5).abs() < 1e-6, "重置后 y = (3+0)/2 = 1.5, 实际 {y}");
    }

    #[test]
    fn test_bandpass_basic() {
        let (b, a) = bandpass_biquad(100.0, 200.0, 1000.0);
        assert!(a[0] > 0.0);
        assert!(b.iter().all(|v| v.is_finite()));
        assert!(a.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn test_bandstop_basic() {
        let (b, a) = bandstop_biquad(100.0, 200.0, 1000.0);
        assert!(a[0] > 0.0);
        assert!(b.iter().all(|v| v.is_finite()));
        assert!(a.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn test_preset_to_filter() {
        let f = DigitalFilter::from_preset(FilterPreset::Lowpass {
            cutoff: 100.0,
            sample_rate: 1000.0,
        });
        // 应为 IIR 类型
        assert!(matches!(f.kind(), FilterKind::IIR { .. }));
    }
}
