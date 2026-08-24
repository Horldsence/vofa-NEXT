//! `cmd_graph` — 节点图 + 逻辑分析仪 / 解码事件 Tauri 命令
//!
//! 由 `src-tauri/src/commands/{graph.rs, logic.rs}` 提取而来。

mod compile_queue;
mod derived;
mod graph;
mod inject;
mod logic;

pub use compile_queue::*;
pub use derived::*;
pub use graph::*;
pub use inject::*;
pub use logic::*;

/// `graph:compile` 事件名 — re-export 来自 `notify_events`, 方便 `graph::apply_tab_graph` 调用
pub use notify_events::GRAPH_COMPILE_EVENT;