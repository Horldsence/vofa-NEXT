//! # pipeline — 数据流水线层
//!
//! 从 `state.rs` 提取的数据处理流水线，职责清晰分离：
//!
//! - [`data_plane`][]: 数据平面执行器 — 每 Transport 节点一个读任务:
//!   传输 broadcast → 合批 → 按源 raw 收集 → 沿全局 BytePlan 字节路由
//!   (协议解析 / 帧解码喂入 / 回注发送) → source_frames → 数值平面评估
//! - [`graph_eval`][]: 节点图评估 (按源触发的槽位热路径 + 事件驱动快照评估)
//! - [`decoder_feed`][]: FrameDecoder 状态同步与字节喂入 (按字节边路由)
//! - [`spectrum_sync`][]: 频谱分析器同步
//! - [`dispatcher`][]: 自适应并发分发器 (AdaptiveRate + Channel 推送循环)
//! - [`stream`][]: 统一分片流框架 (StreamSource + 订阅组 + 自动扩缩容)
//! - [`feed_parallel`][]: feed (RX 解析) 段自动并行 (帧对齐切分 + worker 池,
//!   ParallelFeeder 按 Protocol 节点持有)

pub mod data_plane;
pub mod decoder_feed;
pub mod dispatcher;
pub mod feed_parallel;
pub mod filtered_sources;
pub mod graph_eval;
pub mod spectrum_sync;
pub mod stream;
