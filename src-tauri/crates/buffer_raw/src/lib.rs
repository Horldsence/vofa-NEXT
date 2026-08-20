//! # buffer_raw
//!
//! 原始字节收集器 (RawDataCollector) — 固定容量游标式历史缓冲 + 方向/搜索过滤。
//!
//! - [`RawDataCollector`]: 1 MiB 默认容量的环形游标缓冲 (base_index 单调递增)
//! - [`RawDataChunk`] / [`RawDataBatch`] / [`RawDrain`]: 线上传输结构 (base64 编码)
//! - [`DirectionFilter`] / [`SearchPattern`]: 后端过滤条件 (减少前端负载)

mod raw;
mod raw_filter;

pub use raw::{
    RawDataBatch, RawDataChunk, RawDataCollector, RawDataDirection, RawDrain, StoredChunk,
};
pub use raw_filter::{chunk_matches, DirectionFilter, SearchPattern};