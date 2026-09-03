//! # session
//!
//! AI 对话会话持久化 — 多会话 / 历史的所有权在后端, 前端只是薄视图。
//!
//! 职责:
//! - 视图条目 DTO ([`types`]):与前端 `AiViewItem` / `AiToolRun` 字段严格对齐,
//!   会话以"视图条目流"形式落盘 (`ai_chat_sessions.json`)
//! - 会话存储 ([`store`]):`SessionStore` 持有全部会话, 变更即落盘
//!   (目录由命令层传入, 本 crate 不接触 Tauri API)
//! - 历史派生 ([`history`]):视图条目流 → LLM 消息历史, 供对话循环携带上下文
//!
//! 不负责:对话循环与取消 (在 `chat`)、LLM 调用 (在 `provider`)。

pub mod history;
pub mod store;
pub mod types;

pub use history::derive_history;
pub use store::SessionStore;
pub use types::{ChatSession, SessionMeta, ToolRunDto, ViewItemDto, ViewRoleDto};
