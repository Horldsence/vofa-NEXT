//! `pipeline_stream` — VOFA-NEXT 数据流分发层
//!
//! 由 `src-tauri/src/pipeline/{stream.rs, dispatcher.rs}` 合并而成。
//!
//! 两个核心组件:
//! - [`stream`] — 统一分片流框架 (`StreamSource` trait), 大数据流与
//!   小数据流共用同一套订阅协议与分发机制 (分片组 + 自动并发 +
//!   自适应速率推送 + 顺序保证)
//! - [`dispatcher`] — 自适应并发分发器 (`AdaptiveRate` 速率控制 +
//!   `adaptive_channel_loop` 单订阅者推送循环); 也供 ticker 循环使用

pub mod dispatcher;
pub mod stream;

pub use dispatcher::{adaptive_channel_loop, AdaptiveRate};
pub use stream::{
    join_or_create_group, leave_group, sharded_stream_loop, CanStreamSource, DecodedStreamSource,
    GroupMembership, LogicStreamSource, RawDataSource, StreamSource, WaveformSource,
    MAX_STREAM_SHARDS,
};
