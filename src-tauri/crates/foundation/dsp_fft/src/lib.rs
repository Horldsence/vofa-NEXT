//! `dsp_fft` — 实数信号 FFT 频谱分析与振幅谱 IFFT 合成
//!
//! 基于 `realfft` / `rustfft` 的实数 FFT 实现:
//! - [`spectrum::SpectrumAnalyzer`] — 滑动窗口 + FFT + 4 种输出模式
//!   (Magnitude / Power / PSD / Decibel)
//! - [`ifft::IfftState`] — 从复数频谱重建连续时域信号
//!
//! 频谱分析器的 `WindowType` 字段复用 `dsp_window::WindowType`,但在本 crate
//! 通过 `pub use dsp_window::WindowType` 重新暴露,保证调用方路径稳定。
//!
//! Layer 1 — 依赖 `realfft` + `rustfft` + `dsp_window`。

pub mod ifft;
pub mod spectrum;
pub mod streaming;

pub use ifft::IfftState;
pub use spectrum::{SpectrumAnalyzer, SpectrumOutput, SpectrumResult, WindowType};
pub use streaming::{SpectrumFrame, StreamingFft, StreamingIfft, TransformConfig, TransformError};
