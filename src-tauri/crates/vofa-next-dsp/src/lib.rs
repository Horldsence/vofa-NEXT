//! # vofa-next-dsp
//!
//! 数字信号处理 (DSP) 工具库 — 供节点图频域运算使用。
//!
//! 实际实现位于三个子 crate:
//! - [`dsp_window`][]: 窗函数 (Hann / Hamming / Blackman / Rect)
//! - [`dsp_filter`][]: 数字滤波器 (FIR / IIR biquad, 含低通/高通/带通/带阻预设)
//! - [`dsp_fft`][]: 频谱分析 (FFT + 输出模式 Magnitude/Power/PSD/dB) + IFFT 合成
//!
//! 本 crate 仅为兼容 façade,新代码请直接依赖子 crate。

pub use dsp_filter::{
    bandpass_biquad, bandstop_biquad, highpass_biquad, lowpass_biquad, DigitalFilter, FilterKind,
    FilterPreset,
};
pub use dsp_fft::{IfftState, IfftSynth, SpectrumAnalyzer, SpectrumOutput, SpectrumResult};
pub use dsp_window::{apply_window, WindowType};