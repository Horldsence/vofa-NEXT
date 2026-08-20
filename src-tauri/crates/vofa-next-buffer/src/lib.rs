//! # vofa-next-buffer (façade)
//!
//! Stage C 拆分后的纯 `pub use` 聚合层 — 所有实际实现已迁移到 4 个领域 crate:
//!
//! - [`buffer_ring`]: 泛型环形缓冲区
//! - [`buffer_databuffer`]: 多通道时间序列 + 派生通道 + 窗口查询
//! - [`buffer_graph`]: 节点图数据路由 (Edge / NodeGraph / RoutedData)
//! - [`buffer_raw`]: 原始字节收集器 + 方向/搜索过滤
//!
//! 本 crate 保留作为兼容 façade, 让 [`vofa-next-nodes`](vofa-next-nodes) 和
//! app shell 现有调用 (`use vofa_next_buffer::*`) 无需立即切换到新 crate。
//! Stage H 清理时再决定是否彻底删除。
//!
//! 类型 re-export (Stage D.1 后: 所有实现已彻底搬离本 crate, 本文件保持纯 façade)
pub use buffer_databuffer::{DataBuffer, WaveformWindow};
pub use buffer_graph::{Edge, NodeGraph, RoutedData};
pub use buffer_raw::{
    DirectionFilter, RawDataBatch, RawDataChunk, RawDataCollector, RawDataDirection, RawDrain,
    SearchPattern,
};
pub use buffer_ring::RingBuffer;