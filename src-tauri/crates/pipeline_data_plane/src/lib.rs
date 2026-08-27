//! `pipeline_data_plane` — VOFA-NEXT 数据平面执行器
//!
//! 两平面节点图重构下的核心层:
//!
//! - **字节平面** (全局, 事件驱动): 每个 open 的 Transport 节点一个读任务
//!   ([`data_plane::read_task`]),`subscribe → record_rx → 按源 raw 收集 →
//!   沿全局 [`BytePlan`] 推送 ([`data_plane::byte_router::route_bytes`]):
//!   Protocol.in 解析 / FrameDecoder.in 喂入 / Transport.tx 发送;
//!   Protocol 节点的 convert_to 输出引擎把帧重编码为字节继续沿 `out` 边下推。
//!
//! - **数值平面** (每 tab f32 槽位): Protocol 节点产帧 → [`data_plane::frame_dispatch`]
//!   写 source_frames 缓存 + 触发引用该源的 tab 图评估
//!   (见 [`graph_eval::process_source_batch`])。
//!
//! 本 crate 在 `src-tauri` 旧结构中由 `state::app_state` (GraphEvalState) +
//! `pipeline::data_plane/*` + `pipeline::{decoder_feed, feed_parallel, graph_eval}` 合并而成。
//!
//! ## 子模块
//!
//! - [`data_plane`] — 数据平面共享状态 ([`DataPlaneState`]) 与执行子模块
//!   ([`byte_router`], [`frame_dispatch`], [`read_task`], [`reconcile`])
//! - [`decoder_feed`] — FrameDecoder 节点状态缓存与字节喂入
//! - [`feed_parallel`] — feed (RX 解析) 段自动并行编排
//! - [`graph_eval`] — 数值平面评估 (按源触发的热路径 + 事件驱动快照评估)
//! - [`eval_state`] — 共享状态类型 (GraphEvalState / StreamGroupState /
//!   GraphOutputSnapshot / CustomInputBatch / StringOutputSnapshot / SpectrumBatch)

pub mod data_plane;
pub mod decoder_feed;
pub mod eval_state;
pub mod feed_parallel;
pub mod graph_eval;

pub use data_plane::byte_router::RouteSummary;
pub use data_plane::{
    byte_router, frame_dispatch, read_task, reconcile, DataPlaneMetrics, DataPlaneState,
    ProtocolNodeState, METRICS_REPORT_INTERVAL, STATS_THROTTLE_MS,
};
pub use decoder_feed::{
    ensure_decoder, feed_decoder_by_id, feed_one_decoder, sync_decoders_now, DecoderFeedCache,
    DecoderParseConfig,
};
pub use eval_state::{
    build_graph_eval_state, CustomInputBatch, GraphEvalState, GraphOutputSnapshot, SpectrumBatch,
    StreamGroupState, StringOutputSnapshot, DEFAULT_CAN_BUFFER_CAPACITY,
    DEFAULT_CAN_LOAD_STATS_WINDOW, DEFAULT_DECODED_BUFFER_CAPACITY, DEFAULT_LOGIC_BUFFER_CAPACITY,
};
pub use feed_parallel::{
    workers_needed, ParallelFeeder, ParallelTiming, FEED_PARALLEL_UNIT, MAX_FEED_WORKERS,
    MIN_WORKER_BYTES,
};
pub use graph_eval::{evaluate_snapshot_now, process_source_batch, EvalBreakdown};
