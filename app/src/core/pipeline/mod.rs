//! # pipeline — 数据流水线层
//!
//! 从 `state.rs` 提取的数据处理流水线，职责清晰分离：
//!
//! - [`data_loop`][]: 传输层 broadcast → 协议解析 → 缓冲 → 图评估 → 前端推送
//! - [`graph_eval`][]: 节点图评估与派生值收集
//! - [`decoder_feed`][]: FrameDecoder 状态同步与字节喂入
//! - [`spectrum_sync`][]: 频谱分析器同步

mod data_loop;
pub mod decoder_feed;
pub mod graph_eval;
pub mod spectrum_sync;

pub use data_loop::run as data_loop;
