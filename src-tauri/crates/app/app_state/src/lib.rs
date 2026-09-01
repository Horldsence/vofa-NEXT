//! `app_state` — L3 应用核心: 全局状态容器 + 后台推送 ticker + 工作区持久化
//!
//! 持有 TransportManager / DataPlaneState / per-tab CompiledGraph / 源图存储 /
//! 工作区 / 各类缓冲区, 供 L4 命令层借用; 允许依赖 foundation / protocol /
//! transport / node / pipeline 各层, 禁止依赖任何 `cmd_*` 命令 crate。
//!
//! 数据平面的求值状态类型 (GraphEvalState / StreamGroupState / snapshot 批次)
//! 定义在 [`data_plane`], 本 crate 直接引用, 不做 re-export。

mod app_state;
mod runtime;
mod source_graph;
mod tickers;
mod workspace;

pub use app_state::{AppState, WaveformSnapshot};
pub use runtime::{flush_workspace_on_exit, spawn_background_tasks};
pub use source_graph::{SourceGraphs, SourceNodeHint, TabSourceGraph};
pub use tickers::{spectrum_ticker, text_output_ticker, textout_sender_ticker};
pub use workspace::{
    collect_workspace_file, load_workspace, prune_positions, save_workspace, workspace_path,
    DataTabMeta, Position, TabGraphFile, TabMeta, WidgetRecord, WorkspaceFile, WorkspaceInner,
    WorkspaceState, WORKSPACE_FILE_NAME,
};
