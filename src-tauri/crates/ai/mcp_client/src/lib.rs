//! # mcp_client
//!
//! MCP 客户端 — 连接外部 MCP server (stdio 子进程 / streamable-http),
//! 聚合其工具供 AI 对话调用;配置持久化为 app config dir 下的
//! `mcp_servers.json` (路径由命令层传入,本 crate 不依赖 Tauri)。
//!
//! 工具命名:多 server 工具可能重名,聚合时统一加前缀
//! `mcp_{server}_{tool}` ([`manager::McpManager::list_tools`] 返回的
//! `ToolSpecDto.name` 即前缀名,调用侧无感知)。

pub mod manager;
pub mod store;
pub mod types;

pub use manager::{McpManager, CONNECTION_TIMEOUT_SECS};
pub use store::{load_servers, save_servers};
pub use types::{McpServerConfig, McpToolInfo, McpTransport};
