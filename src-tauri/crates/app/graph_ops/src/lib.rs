//! `graph_ops` — 图提交核心 (无 Tauri IPC)
//!
//! 层级: L3 app (应用核心)。被 L4 命令层 [`graph`] 与 MCP 工具 [`mcp_server`]
//! 共用同一条提交路径; 允许依赖 foundation / protocol / transport / node /
//! pipeline 各层与同层的 `app_state`, 禁止依赖任何 `cmd_*` 命令 crate。
//!
//! 职责:
//! - [`apply::apply_tab_graph_parts`] — tab 图整体编译提交 (乐观并发检查 →
//!   ProtocolSource 注入 → 编译 → 全局 BytePlan 重建 → 原子提交 → 快照评估)
//! - [`source_graph`] — 连线/删线拓扑 op (默认 handle 解析 + RawData 改写 + 幂等)
//! - [`derived`] — 图派生数据 (输出端口表 / 生效通道数, 后端单一权威)
//! - [`inject`] — ProtocolSource NodeDef 自动注入
//! - [`compile_queue`] — per-tab last-write-wins 编译队列 (状态事件广播)
//!
//! 事件契约: `graph:derived` / `graph:compile` / `graph:source` (事件名常量在
//! [`notify_events`], payload 类型在本 crate)。

mod apply;
mod compile_queue;
mod derived;
mod inject;
mod source_graph;

pub use apply::{apply_remove_tab_graph, apply_tab_graph, apply_tab_graph_parts};
pub use compile_queue::*;
pub use derived::*;
pub use inject::*;
pub use source_graph::{
    apply_connect_edge, apply_disconnect_edge, ConnectedEdge, DisconnectedEdge, GraphSourceEvent,
    GRAPH_SOURCE_EVENT,
};

/// `graph:compile` 事件名 — re-export 自 `notify_events`, 方便调用方统一取用
pub use notify_events::GRAPH_COMPILE_EVENT;
