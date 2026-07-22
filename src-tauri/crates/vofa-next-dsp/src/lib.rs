//! # vofa-next-dsp
//!
//! 数字信号处理 (DSP) 工具库 — 供节点图频域运算使用。
//!
//! - [`window`][]: 窗函数 (Hann / Hamming / Blackman / Rect)
//! - [`spectrum`][]: 频谱分析 (FFT + 输出模式 Magnitude/Power/PSD/dB)
//! - [`filter`][]: 数字滤波器 (FIR / IIR biquad, 含低通/高通/带通/带阻预设)

pub mod filter;
pub mod spectrum;
pub mod window;

pub use filter::{DigitalFilter, FilterKind, FilterPreset};
pub use spectrum::{SpectrumAnalyzer, SpectrumOutput, SpectrumResult, WindowType};
pub use window::apply_window;
