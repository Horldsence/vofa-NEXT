//! # state — 应用全局状态与后台循环
//!
//! - [`app_state`][]: 类型定义：AppState、GraphEvalState、快照结构
//! - [`tickers`][]: 后台推送循环：图输出/Custom输入/频谱/CAN帧/原始数据

mod app_state;
mod tickers;

pub use app_state::{
    AppState, CustomInputBatch, GraphEvalState, GraphOutputSnapshot, SpectrumBatch,
    StreamGroupState,
};
pub use tickers::{custom_input_ticker, graph_output_ticker, spectrum_ticker};

use parking_lot::{Mutex, RwLock};
use std::sync::Arc;
use tauri::AppHandle;
use vofa_next_buffer::{DataBuffer, RawDataCollector};
use vofa_next_core::{CanBuffer, CanLoadStats, DecodedBuffer, LogicBuffer, PipelineConfig};
use vofa_next_protocol::ProtocolEngine;

/// 数据循环 — 委托到 pipeline::data_loop
///
/// 架构 (节点图后端化重构):
/// - data_loop (本函数): 只做 recv + mpsc.send, 最快消费 broadcast 避免 Lagged
/// - parse_task: 从 mpsc 收数据, 做 协议解析 + buffer.push + 图评估
///   - 帧/CAN/逻辑数据不再逐包 emit, 统一由 Channel ticker 周期快照推送
///   - 统计节流: STATS_THROTTLE_MS 内累积, 一次性 emit `transport:rx`
///   - 图评估: 调用 process_frames_batch 按包批量计算所有节点输出
///     结果存入 output_snapshot, 由独立的自适应 ticker task 推送到前端
#[allow(clippy::too_many_arguments)]
pub async fn data_loop(
    app: AppHandle,
    rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
    protocol: Arc<Mutex<Box<dyn ProtocolEngine>>>,
    buffer: Arc<Mutex<DataBuffer>>,
    eval_state: GraphEvalState,
    raw_data_collector: Arc<Mutex<RawDataCollector>>,
    can_buffer: Arc<Mutex<CanBuffer>>,
    can_load_stats: Arc<Mutex<CanLoadStats>>,
    logic_buffer: Arc<Mutex<LogicBuffer>>,
    decoded_buffer: Arc<Mutex<DecodedBuffer>>,
    config: Arc<RwLock<PipelineConfig>>,
) {
    crate::pipeline::data_loop(
        app,
        rx,
        protocol,
        buffer,
        eval_state,
        raw_data_collector,
        can_buffer,
        can_load_stats,
        logic_buffer,
        decoded_buffer,
        config,
    )
    .await;
}
