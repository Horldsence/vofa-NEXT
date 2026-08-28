//! `app_state` — VOFA-NEXT 应用全局状态 + 后台推送 ticker
//!
//! 由 `src-tauri/src/state/{app_state.rs, tickers.rs, mod.rs}` 提取而成。
//!
//! `GraphEvalState` / `StreamGroupState` / 4 个 snapshot 类型
//! (`GraphOutputSnapshot` / `CustomInputBatch` / `StringOutputSnapshot` /
//! `SpectrumBatch`) 定义在 [`pipeline_data_plane`] (数据平面下沿), 本 crate
//! 仅 `pub use` 重新暴露以保持 `crate::state::*` 旧调用路径可用。
//!
//! 新代码应直接依赖 `pipeline_data_plane` 拿这些类型, 老 `src-tauri` 命令经
//! 由 `crate::state::*` 间接 re-export 维持兼容。

pub use pipeline_data_plane::{
    build_graph_eval_state, CustomInputBatch, GraphEvalState, GraphOutputSnapshot, SpectrumBatch,
    StreamGroupState, StringOutputSnapshot, DEFAULT_CAN_BUFFER_CAPACITY,
    DEFAULT_CAN_LOAD_STATS_WINDOW, DEFAULT_DECODED_BUFFER_CAPACITY, DEFAULT_LOGIC_BUFFER_CAPACITY,
};

mod app_state;
mod tickers;

pub use app_state::AppState;
pub use tickers::{
    custom_input_ticker, graph_output_ticker, spectrum_ticker, text_output_ticker,
    textout_sender_ticker,
};
