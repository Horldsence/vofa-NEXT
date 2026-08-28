//! # ai_provider
//!
//! AI 对话 LLM provider 聚合层 — 封装 [`genai`],向上层 (`ai_chat`) 提供与
//! 具体 provider 无关的统一对话接口。
//!
//! 职责:
//! - IPC DTO 类型定义 ([`types`]):与前端 `src/types/ai.ts` 字段严格对齐
//! - provider 适配器注册表:适配器字符串 → genai `AdapterKind`
//! - 客户端构建 ([`client`]):按请求注入 api key (AuthResolver) 与自定义
//!   base_url (ServiceTargetResolver,接入任意 OpenAI 兼容服务)
//! - 流式对话 ([`client`]):genai 流事件 → [`ProviderEvent`] (文本增量 / 推理
//!   增量 / 回合结束含聚合文本与工具调用)
//!
//! 不负责:多轮工具调用循环 (在 `ai_chat`)、MCP 工具来源 (在 `mcp_client`)。

pub mod client;
pub mod types;

pub use client::{chat_turn_stream, build_client, validate_config};
pub use types::{
    adapter_kind_from_str, list_adapters, AdapterInfo, AiProviderConfig, ChatMessageDto,
    ChatRoleDto, ChatTurnOutcome, ProviderEvent, ToolCallDto, ToolSpecDto,
};
