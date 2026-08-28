//! AI 领域错误 — LLM provider 调用、对话编排与 MCP 桥接的强类型错误。
//!
//! 设计与 [`crate::config`] 一致:字符串仅作为结构化字段承载真实数据
//! (`adapter` / `model` / `tool` 等),不做 catch-all。

use crate::{Boxed, Error};
use thiserror::Error as ThisError;

/// LLM provider 层错误 (genai 封装与消息转换)。
#[derive(Debug, ThisError)]
pub enum AiError {
    /// 设置未配置 API key 时发起对话。
    #[error("provider [{adapter}] 缺少 API key")]
    MissingApiKey {
        /// 目标 provider 适配器标识 (如 `openai` / `anthropic`)。
        adapter: String,
    },

    /// 适配器标识无法映射到已支持的 provider。
    #[error("未知 provider 适配器: {adapter}")]
    UnknownAdapter {
        /// 用户配置的适配器字符串。
        adapter: String,
    },

    /// `openai_compatible` 适配器必须提供自定义 base_url。
    #[error("openai_compatible 适配器缺少 base_url")]
    MissingBaseUrl,

    /// 配置的模型名为空。
    #[error("provider [{adapter}] 未配置模型名")]
    MissingModel {
        /// 目标 provider 适配器标识。
        adapter: String,
    },

    /// 对话请求或流式读取失败 (网络 / 鉴权 / 限流等)。
    #[error("LLM 请求失败 [{adapter}/{model}]: {source}")]
    ProviderRequest {
        /// 目标 provider 适配器标识。
        adapter: String,
        /// 目标模型名。
        model: String,
        /// 底层 genai 错误。
        #[source]
        source: Boxed,
    },

    /// 对话任务被用户取消。
    #[error("对话任务已取消")]
    Cancelled,

    /// 工具调用循环超过最大轮次仍未得到最终回答。
    #[error("工具调用循环超过最大轮次 ({rounds})")]
    MaxToolRounds {
        /// 实际达到的轮次上限。
        rounds: u32,
    },

    /// 指定 id 的对话会话不存在。
    #[error("AI 会话 [{id}] 不存在")]
    UnknownSession {
        /// 会话 id。
        id: String,
    },

    /// 会话历史文件读写失败。
    #[error("AI 会话持久化失败: {source}")]
    Persist {
        /// 底层 IO 错误。
        #[source]
        source: std::io::Error,
    },
}

impl Error for AiError {
    fn kind(&self) -> &'static str {
        match self {
            Self::MissingApiKey { .. } => "AiMissingApiKey",
            Self::UnknownAdapter { .. } => "AiUnknownAdapter",
            Self::MissingBaseUrl => "AiMissingBaseUrl",
            Self::MissingModel { .. } => "AiMissingModel",
            Self::ProviderRequest { .. } => "AiProviderRequest",
            Self::Cancelled => "AiCancelled",
            Self::MaxToolRounds { .. } => "AiMaxToolRounds",
            Self::UnknownSession { .. } => "AiUnknownSession",
            Self::Persist { .. } => "AiPersist",
        }
    }
}

/// MCP 桥接错误 (client 连接外部 server / 本地 server 生命周期)。
#[derive(Debug, ThisError)]
pub enum McpError {
    /// server 配置不合法 (缺命令 / URL 无法解析等)。
    #[error("MCP server [{name}] 配置无效: {details}")]
    InvalidConfig {
        /// 配置项名称。
        name: String,
        /// 具体无效原因。
        details: String,
    },

    /// 连接外部 MCP server 失败。
    #[error("MCP server [{name}] 连接失败: {source}")]
    Connect {
        /// 配置项名称。
        name: String,
        /// 底层传输错误。
        #[source]
        source: Boxed,
    },

    /// 指定 server 尚未连接 (或已断开)。
    #[error("MCP server [{id}] 未连接")]
    NotConnected {
        /// server 配置 id。
        id: String,
    },

    /// 指定 id 的 server 配置不存在。
    #[error("MCP server [{id}] 不存在")]
    UnknownServer {
        /// server 配置 id。
        id: String,
    },

    /// 工具调用被远端标记为失败 (is_error)。
    #[error("MCP 工具 [{tool}] 调用失败: {details}")]
    ToolFailed {
        /// 工具名。
        tool: String,
        /// 远端返回的错误文本。
        details: String,
    },

    /// server 配置文件读写失败。
    #[error("MCP 配置持久化失败: {source}")]
    Persist {
        /// 底层 IO 错误。
        #[source]
        source: std::io::Error,
    },

    /// 本地 MCP server 端口被占用等启动失败。
    #[error("本地 MCP server 启动失败 (端口 {port}): {source}")]
    ServerStart {
        /// 监听端口。
        port: u16,
        /// 底层 IO 错误。
        #[source]
        source: Boxed,
    },

    /// 本地 MCP server 未启动时执行停止/查询。
    #[error("本地 MCP server 未启动")]
    ServerNotRunning,
}

impl Error for McpError {
    fn kind(&self) -> &'static str {
        match self {
            Self::InvalidConfig { .. } => "McpInvalidConfig",
            Self::Connect { .. } => "McpConnect",
            Self::NotConnected { .. } => "McpNotConnected",
            Self::UnknownServer { .. } => "McpUnknownServer",
            Self::ToolFailed { .. } => "McpToolFailed",
            Self::Persist { .. } => "McpPersist",
            Self::ServerStart { .. } => "McpServerStart",
            Self::ServerNotRunning => "McpServerNotRunning",
        }
    }
}
