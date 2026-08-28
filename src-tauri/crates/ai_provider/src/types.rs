//! AI 对话 IPC DTO 与 provider 适配器注册表。
//!
//! serde 字段为 snake_case,与前端 `src/types/ai.ts` 严格对齐。

use error::AiError;
use genai::adapter::AdapterKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 对话消息角色 (serde 小写,与前端对齐)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRoleDto {
    /// 系统提示词。
    System,
    /// 用户输入。
    User,
    /// 助手回复 (可携带工具调用)。
    Assistant,
    /// 工具执行结果 (回填给 LLM)。
    Tool,
}

/// 工具调用描述 — 助手消息中 LLM 发起的调用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDto {
    /// 调用唯一 id (与 tool 结果消息的 `tool_call_id` 对应)。
    pub id: String,
    /// 工具名。
    pub name: String,
    /// JSON 参数对象 (LLM 生成)。
    pub arguments: Value,
}

/// 对话消息 DTO — 前端历史与工具结果回填共用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageDto {
    /// 消息角色。
    pub role: ChatRoleDto,
    /// 文本内容 (assistant 可为空串,仅有工具调用)。
    #[serde(default)]
    pub content: String,
    /// assistant 消息携带的工具调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallDto>>,
    /// tool 消息对应的调用 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// tool 消息对应的工具名 (部分 provider 回填需要)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// 工具规格 — 传给 LLM 的工具定义 (JSON Schema 入参)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpecDto {
    /// 工具名 (LLM 调用时引用)。
    pub name: String,
    /// 工具用途描述。
    #[serde(default)]
    pub description: String,
    /// 入参 JSON Schema。
    #[serde(default)]
    pub input_schema: Value,
}

/// LLM provider 配置 — 随每次对话请求从设置传入,后端不持久化密钥。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderConfig {
    /// 适配器标识,见 [`list_adapters`] (如 `openai` / `deepseek` / `openai_compatible`)。
    pub adapter: String,
    /// 自定义 base_url;空串表示用 provider 默认端点。
    #[serde(default)]
    pub base_url: String,
    /// API key。
    #[serde(default)]
    pub api_key: String,
    /// 模型名 (如 `gpt-4o-mini` / `deepseek-chat`)。
    pub model: String,
    /// 采样温度 (None = provider 默认)。
    #[serde(default)]
    pub temperature: Option<f64>,
    /// 最大生成 token 数 (None = provider 默认)。
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

/// 适配器元数据 — 设置 UI 下拉与默认端点提示。
#[derive(Debug, Clone, Serialize)]
pub struct AdapterInfo {
    /// 适配器标识。
    pub id: &'static str,
    /// 展示名。
    pub label: &'static str,
    /// 默认 API 端点 (提示用;`openai_compatible` 必须自定义)。
    pub default_base_url: &'static str,
}

/// 支持的适配器注册表 (genai 原生协议)。
pub const ADAPTERS: &[AdapterInfo] = &[
    // OrcaRouter: OpenAI 兼容聚合网关 (模型需 `厂商/模型` 命名, 如 openai/gpt-4o-mini)
    AdapterInfo { id: "orcarouter", label: "OrcaRouter", default_base_url: "https://api.orcarouter.ai/v1" },
    AdapterInfo { id: "openai", label: "OpenAI", default_base_url: "https://api.openai.com/v1" },
    AdapterInfo { id: "anthropic", label: "Anthropic Claude", default_base_url: "https://api.anthropic.com" },
    AdapterInfo { id: "gemini", label: "Google Gemini", default_base_url: "https://generativelanguage.googleapis.com" },
    AdapterInfo { id: "deepseek", label: "DeepSeek", default_base_url: "https://api.deepseek.com" },
    AdapterInfo { id: "moonshot", label: "Moonshot Kimi", default_base_url: "https://api.moonshot.cn/v1" },
    AdapterInfo { id: "zai", label: "智谱 GLM (Z.ai)", default_base_url: "https://api.z.ai/api/paas/v4" },
    AdapterInfo { id: "bigmodel", label: "智谱 BigModel", default_base_url: "https://open.bigmodel.cn/api/paas/v4" },
    AdapterInfo { id: "aliyun", label: "阿里云百炼 (Qwen)", default_base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1" },
    AdapterInfo { id: "openrouter", label: "OpenRouter", default_base_url: "https://openrouter.ai/api/v1" },
    AdapterInfo { id: "groq", label: "Groq", default_base_url: "https://api.groq.com/openai/v1" },
    AdapterInfo { id: "xai", label: "xAI Grok", default_base_url: "https://api.x.ai/v1" },
    AdapterInfo { id: "ollama", label: "Ollama (本地)", default_base_url: "http://localhost:11434" },
    AdapterInfo { id: "openai_compatible", label: "OpenAI 兼容 (自定义端点)", default_base_url: "" },
];

/// 全部适配器元数据 (供设置 UI)。
pub const fn list_adapters() -> &'static [AdapterInfo] {
    ADAPTERS
}

/// 适配器字符串 → genai `AdapterKind`;`openai_compatible` / `orcarouter`
/// 复用 OpenAI 协议。
///
/// # Errors
/// 未知适配器字符串返回 [`AiError::UnknownAdapter`]。
pub fn adapter_kind_from_str(adapter: &str) -> vofa_core::Result<AdapterKind> {
    let kind = match adapter {
        "openai" | "openai_compatible" | "orcarouter" => AdapterKind::OpenAI,
        "openai_responses" => AdapterKind::OpenAIResp,
        "anthropic" => AdapterKind::Anthropic,
        "gemini" => AdapterKind::Gemini,
        "deepseek" => AdapterKind::DeepSeek,
        "moonshot" => AdapterKind::Moonshot,
        "zai" => AdapterKind::Zai,
        "bigmodel" => AdapterKind::BigModel,
        "aliyun" => AdapterKind::Aliyun,
        "openrouter" => AdapterKind::OpenRouter,
        "groq" => AdapterKind::Groq,
        "xai" => AdapterKind::Xai,
        "ollama" => AdapterKind::Ollama,
        _ => {
            return Err(AiError::UnknownAdapter {
                adapter: adapter.to_string(),
            }
            .into())
        }
    };
    Ok(kind)
}

/// 单轮流式回合的最终聚合结果。
#[derive(Debug, Clone, Default)]
pub struct ChatTurnOutcome {
    /// 本轮聚合的全部文本增量。
    pub text: String,
    /// 本轮聚合的推理 (reasoning) 文本。
    pub reasoning: String,
    /// 本轮 LLM 发起的工具调用 (取自 genai 捕获,完整且按序)。
    pub tool_calls: Vec<ToolCallDto>,
}

/// provider 层流式事件 — `ai_chat` 消费并转发给前端。
#[derive(Debug, Clone)]
pub enum ProviderEvent {
    /// 助手文本增量。
    TextDelta(String),
    /// 推理内容增量 (DeepSeek-R1 / Kimi thinking 等)。
    ReasoningDelta(String),
    /// 一轮流结束,携带聚合结果与 token 用量。
    TurnEnd {
        /// 聚合的回合产物。
        outcome: ChatTurnOutcome,
        /// 输入 token 用量 (provider 上报时才有)。
        input_tokens: Option<u64>,
        /// 输出 token 用量 (provider 上报时才有)。
        output_tokens: Option<u64>,
    },
}
