//! # cmd_ai
//!
//! AI 功能 Tauri 命令层 — 对话流式 Channel、MCP client/server 管理、
//! 内置原生工具 (软件自有能力) 与知识库。
//!
//! 状态 ([`commands::AiState`]) 在 `setup` 中 `manage`:
//! - 对话任务注册表 (取消)
//! - 对话会话存储 (多会话 + 历史持久化在 app config dir / `ai_chat_sessions.json`)
//! - MCP client 连接管理器 (配置持久化在 app config dir / `mcp_servers.json`)
//! - 聚合工具缓存 (前端刷新后, 对话按缓存快照选择工具)
//! - 本地 MCP server 句柄 (启停)
//! - 前端托管工具调用的 pending 注册表 (`ai_tool_invoke` 事件桥)
//! - API key 钥匙串存取 (`ai_keychain_*`, 密钥不落 settings.json)

mod commands;
pub mod keychain;
pub mod native_executor;
pub mod skills;

pub use commands::{
    ai_chat_cancel, ai_chat_send, ai_keychain_delete, ai_keychain_get, ai_keychain_set,
    ai_list_providers, ai_tool_resolve, chat_clear_session, chat_create_session,
    chat_delete_session, chat_get_session, chat_list_sessions, chat_rename_session, mcp_add_server,
    mcp_call_tool, mcp_connection_states, mcp_list_servers, mcp_list_tools, mcp_remove_server,
    mcp_server_start, mcp_server_status, mcp_server_stop, mcp_set_server_enabled, AiState,
    McpServerStatus,
};
