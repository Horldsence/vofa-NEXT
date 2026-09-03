//! `dispatcher` — 数值平面辅助
//!
//! 层级: L2 pipeline — 数值平面调度辅助; 允许依赖 L0-L1。
//!
//! 两个子模块:
//! - [`spectrum_sync`] — `sync_spectrum_analyzers` (FrequencyAnalyzer 与 graphs 同步)
//!   与 `sync_ifft_buffers` (Ifft 节点重建缓冲), 由 `app_state::tickers::spectrum_ticker`
//!   每 tick 调用
//! - [`filtered_sources`] — 按源过滤的订阅支持

pub mod filtered_sources;
pub mod spectrum_sync;

pub use spectrum_sync::{sync_ifft_buffers, sync_spectrum_analyzers};
