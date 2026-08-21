//! # state — Stage H 后 thin facade
//!
//! 旧 mod 内容已全部迁至 `crates/app_state/src/` (`AppState`, 4 个后台 ticker)。
//! GraphEvalState / StreamGroupState 与 4 个 snapshot 类型定义在
//! `pipeline_data_plane` crate (由 app_state re-export)。
//!
//! 本文件保持 `crate::state::*` 旧路径有效, 新代码直接 `use app_state::*`。

pub use app_state::{
    custom_input_ticker, graph_output_ticker, spectrum_ticker, text_output_ticker, AppState,
    CustomInputBatch, GraphOutputSnapshot, SpectrumBatch, StringOutputSnapshot,
};
