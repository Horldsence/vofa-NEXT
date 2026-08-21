//! `pipeline_dispatcher` — 数值平面辅助
//!
//! Stage H 拆分: 由 `src-tauri/src/pipeline/{spectrum_sync.rs, filtered_sources.rs}`
//! 合并而成。
//!
//! 两个子模块:
//! - [`spectrum_sync`] — `sync_spectrum_analyzers` (FrequencyAnalyzer 与 graphs 同步)
//!   + `sync_ifft_buffers` (Ifft 节点重建缓冲), 由 `app_state::tickers::spectrum_ticker`
//!   每 tick 调用
//! - [`filtered_sources`] — 按源过滤的订阅支持

pub mod filtered_sources;
pub mod spectrum_sync;

pub use spectrum_sync::{sync_ifft_buffers, sync_spectrum_analyzers};
