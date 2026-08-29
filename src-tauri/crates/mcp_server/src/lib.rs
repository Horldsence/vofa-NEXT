//! # mcp_server
//!
//! 把本应用能力暴露为 MCP server — 外部 AI 客户端 (Claude Desktop / ZCode 等)
//! 可通过 streamable-http 连接 `127.0.0.1:{port}/mcp`,调用串口发送、字节注入、
//! 节点图编辑、波形读取等工具,实现 "AI 控制仪器"。
//!
//! 复用现有能力路径:
//! - 发送走 `TransportManager` (与 `send_raw` 命令同路径)
//! - 图编辑复用 [`cmd_graph::apply_tab_graph`] (解耦 Tauri State;带 `AppHandle`
//!   时 emit `graph:derived`,前端界面实时同步)
//! - 波形读取走 `DataPlaneState::buffer_for` (与 `get_recent_waveform` 同路径)
//!
//! 仅监听 `127.0.0.1` — 不对局域网暴露。

pub mod server;
pub mod tools;

pub use server::{start, McpServerHandle, Toolbox, VofaMcpServer};
