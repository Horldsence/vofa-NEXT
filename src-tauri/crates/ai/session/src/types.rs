//! AI 会话 IPC/持久化 DTO — 与前端 `src/types/ai.ts` 的视图类型严格对齐。
//!
//! 会话以"视图条目流"形式存储 (而非 LLM 消息流):视图条目保留了工具调用卡片
//! 等展示信息, 恢复会话时前端可直接渲染;LLM 历史由 [`crate::history`] 按需派生。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 视图条目角色 — 系统提示词不进视图。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewRoleDto {
    /// 用户输入。
    User,
    /// 助手回复 (可携带工具调用卡片)。
    Assistant,
}

/// 工具调用运行记录 — 与前端 `AiToolRun` 对齐。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRunDto {
    /// 调用唯一 id。
    pub id: String,
    /// 工具名。
    pub name: String,
    /// JSON 参数 (LLM 生成)。
    pub arguments: Value,
    /// 执行输出文本。
    #[serde(default)]
    pub content: String,
    /// 是否执行失败。
    #[serde(default)]
    pub is_error: bool,
    /// 是否已收到结果。
    #[serde(default)]
    pub done: bool,
}

/// 对话视图条目 — 与前端 `AiViewItem` 对齐。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewItemDto {
    /// 条目角色。
    pub role: ViewRoleDto,
    /// 文本内容。
    #[serde(default)]
    pub text: String,
    /// 助手条目携带的工具调用记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolRunDto>>,
    /// 错误条目标记 (仅 UI 展示, 不入 LLM 历史)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<bool>,
    /// 错误种类 ([`error::AppError::kind()`]) — 前端按此本地化展示。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
    /// 错误结构化字段 (adapter / model / rounds 等, 供本地化插值)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_data: Option<BTreeMap<String, String>>,
}

/// 对话会话 — 持久化与恢复的完整单元。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatSession {
    /// 会话唯一 id。
    pub id: String,
    /// 会话标题。
    pub title: String,
    /// 创建时间 (unix 毫秒)。
    pub created_at: u64,
    /// 最近活动时间 (unix 毫秒)。
    pub updated_at: u64,
    /// 视图条目流。
    #[serde(default)]
    pub items: Vec<ViewItemDto>,
}

/// 会话列表元数据 — 列表 UI 只需摘要, 不携带全部条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionMeta {
    /// 会话唯一 id。
    pub id: String,
    /// 会话标题。
    pub title: String,
    /// 创建时间 (unix 毫秒)。
    pub created_at: u64,
    /// 最近活动时间 (unix 毫秒)。
    pub updated_at: u64,
    /// 视图条目数。
    pub item_count: usize,
}
