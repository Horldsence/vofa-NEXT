//! `dsp_filter` — 数字滤波器 (FIR / IIR biquad)
//!
//! 提供 FIR 与 IIR biquad 两种滤波器形式,以及 4 种常用预设
//! (低通/高通/带通/带阻)。Layer 0 — 无 FFT 依赖,可独立编译。
//!
//! 上层 `vofa-next-dsp` façade 暴露 [`DigitalFilter`] / [`FilterKind`] /
//! [`FilterPreset`] 与 4 个 biquad 系数函数给节点图与状态层。

pub mod filter;
pub use filter::{
    bandpass_biquad, bandstop_biquad, highpass_biquad, lowpass_biquad, DigitalFilter, FilterKind,
    FilterPreset,
};