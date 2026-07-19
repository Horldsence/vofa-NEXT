//! # state — 应用全局状态与后台循环
//!
//! - [`app_state`]: 类型定义：AppState、GraphEvalState、快照结构
//! - [`tickers`]: 后台推送循环：图输出/Custom输入/频谱/CAN帧/原始数据

mod app_state;
mod tickers;

pub use app_state::{
    AppState, CustomInputBatch, GraphEvalState, GraphOutputSnapshot, SpectrumBatch,
};
pub use tickers::{
    can_frames_loop, custom_input_ticker, graph_output_ticker, rawdata_loop, spectrum_ticker,
};

use parking_lot::Mutex;
use std::sync::Arc;
use tauri::AppHandle;
use vofa_next_buffer::{DataBuffer, RawDataCollector};
use vofa_next_core::{CanBuffer, CanLoadStats, DecodedBuffer, LogicBuffer};
use vofa_next_protocol::ProtocolEngine;

/// 数据循环 — 委托到 pipeline::data_loop
///
/// 架构 (节点图后端化重构):
/// - data_loop (本函数): 只做 recv + mpsc.send, 最快消费 broadcast 避免 Lagged
/// - parse_task: 从 mpsc 收数据, 做 协议解析 + buffer.push + 图评估 + 批量 emit
///   - 批量 emit: `transport:frames` (数组) 替代每帧一次 `transport:frame`
///   - 统计节流: STATS_THROTTLE_MS 内累积, 一次性 emit
///   - 图评估: 调用 evaluate_all_graphs_with 实时计算所有节点输出
///     结果存入 output_snapshot, 由独立的 60 FPS ticker task 推送到前端
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
    )
    .await;
}
