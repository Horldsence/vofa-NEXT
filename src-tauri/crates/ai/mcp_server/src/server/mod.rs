//! MCP server 实现 — 工具箱抽象、工具 handler 与 HTTP 生命周期。
//!
//! 工具 handler 操作从 [`AppState`] 拆出的 [`Toolbox`] (各字段本就是
//! `Arc` 共享句柄,与 Tauri 管理的是同一份状态);图提交复用
//! [`graph_ops::apply_tab_graph_parts`] (L3 应用核心, 与 L4 命令层同一实现)。
//! 工具具体实现统一在 [`crate::tools`] (内置 AI 原生工具执行器共用),此处
//! 仅做参数包装与错误映射。

mod handlers;
mod params;
mod runtime;
mod toolbox;

pub use handlers::VofaMcpServer;
pub use runtime::{start, McpServerHandle};
pub use toolbox::{Toolbox, MCP_ENDPOINT_PATH};
