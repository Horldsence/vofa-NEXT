//! # cmd_ai
//!
//! AI 功能 Tauri 命令层 — 对话流式 Channel、MCP client/server 管理。
//!
//! 状态 ([`commands::AiState`]) 在 `setup` 中 `manage`:
//! - 对话任务注册表 (取消)
//! - MCP client 连接管理器 (配置持久化在 app config dir / `mcp_servers.json`)
//! - 聚合工具缓存 (前端刷新后, 对话按缓存快照选择工具)
//! - 本地 MCP server 句柄 (启停)

mod commands;

pub use commands::{
    AiState, McpServerStatus, ai_chat_cancel, ai_chat_send, ai_list_providers, mcp_add_server,
    mcp_call_tool, mcp_connection_states, mcp_list_servers, mcp_list_tools, mcp_remove_server,
    mcp_server_start, mcp_server_status, mcp_server_stop, mcp_set_server_enabled,
};
