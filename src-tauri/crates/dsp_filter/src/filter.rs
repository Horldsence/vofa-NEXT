//! 数字滤波器 — FIR (有限脉冲响应) 与 IIR biquad (二阶无限脉冲响应)
//!
//! 含预设模板: 低通/高通/带通/带阻 (Butterworth-style biquad)
//!
//! 用法:
//! ```
//! use dsp_filter::{DigitalFilter, FilterPreset};
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
///
/// 采样率无效 (≤0 / NaN / Inf) 时返回 π/2 (RBJ 有效区间中部, cos=0/sin=1),
/// 避免除零产生 NaN/Inf 系数。
fn w0(cutoff: f32, sample_rate: f32) -> f32 {
    if !sample_rate.is_finite() || sample_rate <= 0.0 {
        return PI_F32 * 0.5;
    }
    2.0 * PI_F32 * cutoff / sample_rate
}

/// 将滤波器频率参数限制在 (0, Nyquist) 开区间内。
///
/// RBJ biquad 系数公式仅在 0 < w0 < π (即 freq < sample_rate/2) 时有效;
/// 越界会使 α = sin(w0)/(2Q) 变号甚至为负, a0 = 1+α 可能 ≤ 1,
/// 极点半平面位置偏移, 严重时滤波器发散。此处把越界/非法输入收敛到稳定范围,
/// 保证任意用户输入都产出有限且稳定的 biquad 系数。
#[allow(clippy::cast_precision_loss)]
fn clamp_to_nyquist(freq: f32, sample_rate: f32) -> f32 {
    if !sample_rate.is_finite() || sample_rate <= 0.0 {
        return if freq.is_finite() && freq > 0.0 {
            freq
        } else {
            1.0
        };
    }
    let nyquist = sample_rate * 0.5;
    if !freq.is_finite() {
        return nyquist * 0.1;
    }
    freq.clamp(nyquist * 0.001, nyquist * 0.999)
}

/// alpha = sin(w0) / (2 * Q)
fn alpha(w0: f32, q: f32) -> f32 {
    w0.sin() / (2.0 * q)
}

/// 低通 biquad 系数 (fc=截止频率, fs=采样率)
pub fn lowpass_biquad(cutoff: f32, sample_rate: f32) -> ([f32; 3], [f32; 3]) {
    let cutoff = clamp_to_nyquist(cutoff, sample_rate);
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
    let cutoff = clamp_to_nyquist(cutoff, sample_rate);
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
    let low = clamp_to_nyquist(low, sample_rate);
    let high = clamp_to_nyquist(high, sample_rate);
    // 几何中心 fc = sqrt(low*high) 要求 low < high, 用户可能填反
    let (low, high) = if low > high { (high, low) } else { (low, high) };
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
    let low = clamp_to_nyquist(low, sample_rate);
    let high = clamp_to_nyquist(high, sample_rate);
    // 几何中心 fc = sqrt(low*high) 要求 low < high, 用户可能填反
    let (low, high) = if low > high { (high, low) } else { (low, high) };
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

// ============ 预设配置 DTO (IPC 唯一事实源) ============
//
// 前端 FilterConfig 原始形态 (preset + cutoff/low/high + sample_rate) 通过
// `update_tab_graph` 同步到后端。后端 `filter_kind_from_config` 在编译期
// 派生 FilterKind (biquad 系数), IIR 的 [b, a] 不再经 IPC 流转。
//
// 序列化约定:
// - `preset` 与前端 FilterConfig.preset (lowercase) 对齐: "lowpass" / "highpass"
//   / "bandpass" / "bandstop" / "fir"
// - "fir" 走 FIR 自由系数 (b 直接传入), 与 4 预设并列; 此 DTO 同时是 FIR 的 IPC 形态
// - snake_case 字段命名; 前端 TS DTO 用同名 (无 rename) 表示
// - `id`/`label`/`precision` 仅为前端 UI 用, 不参与后端 biquad 计算, **不**纳入此 DTO

/// 滤波器配置 — 前端 Filter widget params 直接下发, 后端派生 FilterKind
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "preset", rename_all = "lowercase")]
pub enum FilterConfig {
    /// 低通 — 单一截止频率
    Lowpass {
        cutoff: f32,
        sample_rate: f32,
    },
    /// 高通 — 单一截止频率
    Highpass {
        cutoff: f32,
        sample_rate: f32,
    },
    /// 带通 — 通带 [low, high]
    Bandpass {
        low: f32,
        high: f32,
        sample_rate: f32,
    },
    /// 带阻 (陷波) — 阻带 [low, high]
    Bandstop {
        low: f32,
        high: f32,
        sample_rate: f32,
    },
}

/// FilterConfig 派生 FilterKind (biquad 系数)
/// 低通/高通/带通/带阻 走对应预设; 未知变体走兜底低通 (防御)
pub fn filter_kind_from_config(cfg: &FilterConfig) -> FilterKind {
    match cfg {
        FilterConfig::Lowpass { cutoff, sample_rate } => {
            let (b, a) = lowpass_biquad(*cutoff, *sample_rate);
            FilterKind::IIR { b, a }
        }
        FilterConfig::Highpass { cutoff, sample_rate } => {
            let (b, a) = highpass_biquad(*cutoff, *sample_rate);
            FilterKind::IIR { b, a }
        }
        FilterConfig::Bandpass { low, high, sample_rate } => {
            let (b, a) = bandpass_biquad(*low, *high, *sample_rate);
            FilterKind::IIR { b, a }
        }
        FilterConfig::Bandstop { low, high, sample_rate } => {
            let (b, a) = bandstop_biquad(*low, *high, *sample_rate);
            FilterKind::IIR { b, a }
        }
    }
}

// ============================================================
// JSON 契约测试 (IPC): 前端 `toNodeFilterConfig` 下发 snake_case,
// 与后端 `FilterConfig` 字段命名一致 (rename_all 仅影响 variant 名)
// ============================================================

#[cfg(test)]
mod ipc_serde_tests {
    use super::*;

    #[test]
    fn filter_config_lowpass_snake_case_round_trip() {
        // 前端 IPC 形态: { "preset": "lowpass", "cutoff": ..., "sample_rate": ... }
        let json = r#"{"preset":"lowpass","cutoff":100.0,"sample_rate":1000.0}"#;
        let cfg: FilterConfig = serde_json::from_str(json).expect("snake_case 应反序列化");
        match cfg {
            FilterConfig::Lowpass { cutoff, sample_rate } => {
                assert_eq!(cutoff, 100.0);
                assert_eq!(sample_rate, 1000.0);
            }
            _ => panic!("期望 Lowpass 变体"),
        }
    }

    #[test]
    fn filter_config_bandpass_all_fields_snake_case() {
        let json = r#"{"preset":"bandpass","low":50.0,"high":150.0,"sample_rate":1000.0}"#;
        let cfg: FilterConfig = serde_json::from_str(json).expect("带通 snake_case 应反序列化");
        assert!(matches!(cfg, FilterConfig::Bandpass { .. }));
    }

    #[test]
    fn filter_config_camel_case_sample_rate_rejected() {
        // 关键防御: 前端若误用 camelCase (sampleRate) 必须直接报错,
        // 不能与 snake_case 字段名混淆 (rename_all 不影响字段名)
        let json_camel = r#"{"preset":"lowpass","cutoff":100.0,"sampleRate":1000.0}"#;
        let res: Result<FilterConfig, _> = serde_json::from_str(json_camel);
        assert!(
            res.is_err(),
            "camelCase 不应通过反序列化: 仍能误判为有效输入时即契约漂移"
        );
    }

    #[test]
    fn filter_config_serializes_snake_case() {
        let cfg = FilterConfig::Highpass { cutoff: 200.0, sample_rate: 1000.0 };
        let j = serde_json::to_string(&cfg).unwrap();
        assert!(j.contains("\"sample_rate\""), "字段名应为 sample_rate, 实际: {j}");
        assert!(j.contains("\"preset\":\"highpass\""), "variant 名应小写");
    }

    #[test]
    fn filter_kind_from_config_matches_lowpass_biquad() {
        // IPC 派生产物与现有 lowpass_biquad 一致 (防回归)
        let cfg = FilterConfig::Lowpass { cutoff: 100.0, sample_rate: 1000.0 };
        let expected_kind = FilterKind::IIR {
            b: lowpass_biquad(100.0, 1000.0).0,
            a: lowpass_biquad(100.0, 1000.0).1,
        };
        assert_eq!(filter_kind_from_config(&cfg), expected_kind);
    }
}
