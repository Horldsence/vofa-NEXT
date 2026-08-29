//! `cmd_graph` — 节点图 + 逻辑分析仪 / 解码事件 Tauri 命令
//!
//! 由 `src-tauri/src/commands/{graph.rs, logic.rs}` 提取而来。

mod compile_queue;
mod derived;
mod graph;
mod hir_query;
mod inject;
mod logic;
mod source_graph;
mod workspace;

pub use compile_queue::*;
pub use derived::*;
pub use graph::*;
pub use hir_query::*;
pub use inject::*;
pub use logic::*;
pub use source_graph::{
    apply_connect_edge, apply_disconnect_edge, connect_edge, disconnect_edge, get_source_graph,
    ConnectedEdge, DisconnectedEdge, GraphSourceEvent, GRAPH_SOURCE_EVENT,
};
pub use workspace::{
    restore_workspace, workspace_get, workspace_set_tabs, TabGraphSnapshot, WorkspaceSnapshot,
};

/// `graph:compile` 事件名 — re-export 来自 `notify_events`, 方便 `graph::apply_tab_graph` 调用
pub use notify_events::GRAPH_COMPILE_EVENT;
