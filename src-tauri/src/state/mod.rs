//! # state — 应用全局状态与后台循环
//!
//! - [`app_state`][]: 类型定义：AppState、GraphEvalState、快照结构
//! - [`tickers`][]: 后台推送循环：图输出/Custom输入/频谱/CAN帧/原始数据
//!
//! 数据流水线 (传输 broadcast → 字节路由 → 协议解析 → 图评估) 已迁至
//! [`crate::pipeline::data_plane`] (每 Transport 节点一个读任务)。

mod app_state;
mod tickers;

pub use app_state::{
    AppState, CustomInputBatch, GraphEvalState, GraphOutputSnapshot, SpectrumBatch,
    StreamGroupState, StringOutputSnapshot,
};
pub use tickers::{
    custom_input_ticker, graph_output_ticker, spectrum_ticker, text_output_ticker,
};
