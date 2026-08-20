//! `dsp_window` — 窗函数 (FFT 前置处理)
//!
//! 提供 4 种常用窗 (Rect / Hann / Hamming / Blackman) 的系数计算与相干增益,
//! 与 [`apply_window`] 原地应用函数。Layer 0 — 无 FFT 依赖,可独立编译。
//!
//! 上层 `dsp_fft` 在 FFT 前调用 [`apply_window`];`vofa-next-dsp` façade
//! 把 [`WindowType`] 与 [`apply_window`] 暴露给节点图与状态层。
//!
//! # 示例
//! ```
//! use dsp_window::{apply_window, WindowType};
//! let mut data = vec![1.0; 1024];
//! apply_window(&WindowType::Hann, &mut data);
//! assert!(data[0] < 0.01);
//! assert!((data[512] - 1.0).abs() < 0.01);
//! ```

pub mod window;
pub use window::{apply_window, WindowType};