//! `dsp_measure` — 波形测量与自动设置的纯数学内核
//!
//! 面向数据切片 (`&[f32]` + 采样间隔) 的信号测量原语, 不感知缓冲/IPC/前端:
//! - [`stats::channel_stats`] — Vpp/极值/均值/RMS(含直流与去直流)/占空比
//! - [`period::detect_period`] — 周期/频率检测 (FFT 自相关为主, 迟滞过零回退)
//! - [`autoset::suggest_autoset`] — 示波器自动设置建议 (时基按周期取档 + V/div 1-2-5)
//!
//! 唯一实现原则: 1-2-5 取档、时基档位表、周期检测都在本 crate 定义,
//! 前端保留的 `TIME_BASES_SEC`/`V_PER_DIV` 仅为 UI 旋钮常量 (数值镜像)。
//!
//! Layer 0 — 依赖 `dsp_fft` (FFT 自相关) + `serde`。

pub mod autoset;
pub mod period;
pub mod stats;

pub use autoset::{suggest_autoset, AutoSetChannel, AutoSetSuggestion, ChannelFit};
pub use period::{detect_period, PeriodEstimate};
pub use stats::{channel_stats, ChannelStats};
