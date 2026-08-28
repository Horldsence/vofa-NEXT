//! genai 客户端构建与流式回合执行。
//!
//! 每次对话按配置构建 [`genai::Client`] (构建开销极小,官方文档即此用法):
//! - API key 经 `AuthResolver` 注入 (Ollama 等本地服务可为空)
//! - 自定义 base_url 经 `ServiceTargetResolver` 覆盖端点,接入任意 OpenAI 兼容服务
//!
//! [`chat_turn_stream`] 执行单轮流式对话,把 genai 流事件归一化为
//! [`ProviderEvent`];文本与工具调用以 `End` 事件的捕获值为准 (完整、按序)。

use error::AiError;
use futures::{Stream, StreamExt};
use genai::chat::{
    ChatMessage as GenaiChatMessage, ChatOptions, ChatRequest, ContentPart, MessageContent,
    StreamEnd, Tool as GenaiTool, ToolCall as GenaiToolCall, ToolResponse as GenaiToolResponse,
};
use genai::resolver::{AuthData, Endpoint};
use genai::{Client, ModelIden, ServiceTarget};
use vofa_core::Result;

use crate::types::{
    adapter_kind_from_str, AiProviderConfig, ChatMessageDto, ChatRoleDto, ChatTurnOutcome,
    ProviderEvent, ToolCallDto, ToolSpecDto,
};

/// Ollama 等本地服务允许空 key。
fn adapter_needs_key(adapter: &str) -> bool {
    !matches!(adapter, "ollama")
}

/// 按配置构建 genai 客户端 (校验适配器 / key / 自定义端点)。
///
/// # Errors
/// 未知适配器 (`AiError::UnknownAdapter`)、非本地 provider 且 key 为空
/// (`AiError::MissingApiKey`)、`openai_compatible` 未提供 base_url
/// (`AiError::MissingBaseUrl`)。
pub fn build_client(cfg: &AiProviderConfig) -> Result<Client> {
    adapter_kind_from_str(&cfg.adapter)?;

    if adapter_needs_key(&cfg.adapter) && cfg.api_key.is_empty() {
        return Err(AiError::MissingApiKey {
            adapter: cfg.adapter.clone(),
        }
        .into());
    }
    if cfg.adapter == "openai_compatible" && cfg.base_url.is_empty() {
        return Err(AiError::MissingBaseUrl.into());
    }

    let api_key = cfg.api_key.clone();
    let mut builder = Client::builder()
        .with_auth_resolver_fn(move |_model_iden| Ok(Some(AuthData::from_single(api_key.clone()))));

    // 自定义端点: 覆盖 resolved target 的 endpoint (OpenAI 兼容网关 / 自建代理)
    if !cfg.base_url.is_empty() {
        let base_url = cfg.base_url.clone();
        builder = builder.with_service_target_resolver_fn(move |mut target: ServiceTarget| {
            target.endpoint = Endpoint::from_owned(base_url.clone());
            Ok(target)
        });
    }

    Ok(builder.build())
}

/// DTO 消息 → genai 消息。
///
/// assistant 允许 "仅工具调用无文本";tool 消息回填 `call_id` 与工具名。
fn to_genai_message(msg: &ChatMessageDto) -> GenaiChatMessage {
    match msg.role {
        ChatRoleDto::System => GenaiChatMessage::system(msg.content.clone()),
        ChatRoleDto::User => GenaiChatMessage::user(msg.content.clone()),
        ChatRoleDto::Assistant => {
            let mut parts: Vec<ContentPart> = Vec::new();
            if !msg.content.is_empty() {
                parts.push(ContentPart::Text(msg.content.clone()));
            }
            if let Some(calls) = &msg.tool_calls {
                parts.extend(calls.iter().map(|c| {
                    ContentPart::ToolCall(GenaiToolCall {
                        call_id: c.id.clone(),
                        fn_name: c.name.clone(),
                        fn_arguments: c.arguments.clone(),
                        thought_signatures: None,
                    })
                }));
            }
            GenaiChatMessage::assistant(MessageContent::from_parts(parts))
        }
        ChatRoleDto::Tool => GenaiChatMessage::tool(MessageContent::from_parts([ContentPart::ToolResponse(
            GenaiToolResponse {
                call_id: msg.tool_call_id.clone().unwrap_or_default(),
                fn_name: msg.name.clone(),
                content: msg.content.clone(),
            },
        )])),
    }
}

/// DTO 工具规格 → genai 工具定义。
fn to_genai_tool(spec: &ToolSpecDto) -> GenaiTool {
    GenaiTool {
        name: genai::chat::ToolName::Custom(spec.name.clone()),
        description: (!spec.description.is_empty()).then(|| spec.description.clone()),
        schema: (!spec.input_schema.is_null()).then(|| spec.input_schema.clone()),
        strict: None,
        config: None,
    }
}

/// genai 工具调用 → DTO。
fn tool_call_to_dto(call: &GenaiToolCall) -> ToolCallDto {
    ToolCallDto {
        id: call.call_id.clone(),
        name: call.fn_name.clone(),
        arguments: call.fn_arguments.clone(),
    }
}

/// `StreamEnd` 捕获内容 → 回合聚合产物。
fn outcome_from_stream_end(end: &StreamEnd) -> ChatTurnOutcome {
    ChatTurnOutcome {
        text: end
            .captured_texts()
            .map(|texts| texts.concat())
            .unwrap_or_default(),
        reasoning: end.captured_reasoning_content.clone().unwrap_or_default(),
        tool_calls: end
            .captured_tool_calls()
            .map(|calls| calls.iter().map(|c| tool_call_to_dto(c)).collect())
            .unwrap_or_default(),
    }
}

/// 执行单轮流式对话回合。
///
/// `system` 为系统提示词 (可选);`messages` 为完整历史 (含上一轮助手工具
/// 调用与工具结果回填);`tools` 非空时启用工具调用。
///
/// # Errors
/// - 配置校验失败 (见 [`build_client`])
/// - 请求发起或流中途失败:[`AiError::ProviderRequest`]
pub async fn chat_turn_stream(
    cfg: &AiProviderConfig,
    system: Option<String>,
    messages: &[ChatMessageDto],
    tools: &[ToolSpecDto],
) -> Result<impl Stream<Item = Result<ProviderEvent>> + Send> {
    let client = build_client(cfg)?;
    let kind = adapter_kind_from_str(&cfg.adapter)?;
    let model = ModelIden::new(kind, cfg.model.clone());

    let mut req = ChatRequest::new(messages.iter().map(to_genai_message).collect());
    if let Some(sys) = system {
        req = req.with_system(sys);
    }
    if !tools.is_empty() {
        req = req.with_tools(tools.iter().map(to_genai_tool));
    }

    let mut opts = ChatOptions::default()
        .with_capture_usage(true)
        .with_capture_content(true)
        .with_capture_tool_calls(true);
    if let Some(t) = cfg.temperature {
        opts = opts.with_temperature(t);
    }
    if let Some(m) = cfg.max_tokens {
        opts = opts.with_max_tokens(m);
    }

    let adapter_label = cfg.adapter.clone();
    let model_label = cfg.model.clone();
    let resp = client
        .exec_chat_stream(model, req, Some(&opts))
        .await
        .map_err(|e| {
            AiError::ProviderRequest {
                adapter: adapter_label.clone(),
                model: model_label.clone(),
                source: Box::new(e),
            }
        })?;

    let err_adapter = cfg.adapter.clone();
    let err_model = cfg.model.clone();
    let mapped = resp.stream.map(move |item| {
        match item {
            // 文本 / 推理增量;工具调用分片不透传 (以 End 捕获的完整调用为准)
            Ok(genai::chat::ChatStreamEvent::Chunk(c)) => {
                Some(Ok(ProviderEvent::TextDelta(c.content)))
            }
            Ok(genai::chat::ChatStreamEvent::ReasoningChunk(c)) => {
                Some(Ok(ProviderEvent::ReasoningDelta(c.content)))
            }
            Ok(genai::chat::ChatStreamEvent::End(end)) => Some(Ok(ProviderEvent::TurnEnd {
                outcome: outcome_from_stream_end(&end),
                input_tokens: end
                    .captured_usage
                    .as_ref()
                    .and_then(|u| u.prompt_tokens)
                    .map(|v| u64::try_from(v.max(0)).unwrap_or_default()),
                output_tokens: end
                    .captured_usage
                    .as_ref()
                    .and_then(|u| u.completion_tokens)
                    .map(|v| u64::try_from(v.max(0)).unwrap_or_default()),
            })),
            Ok(
                genai::chat::ChatStreamEvent::Start
                | genai::chat::ChatStreamEvent::ToolCallChunk(_)
                | genai::chat::ChatStreamEvent::ThoughtSignatureChunk(_),
            ) => None,
            Err(e) => Some(Err(AiError::ProviderRequest {
                adapter: err_adapter.clone(),
                model: err_model.clone(),
                source: Box::new(e),
            }
            .into())),
        }
    });
    Ok(mapped.filter_map(std::future::ready))
}

/// 判断配置是否可用于发起对话 (供前端发送前预检)。
///
/// # Errors
/// 与 [`build_client`] 一致;另校验模型名非空。
pub fn validate_config(cfg: &AiProviderConfig) -> Result<()> {
    adapter_kind_from_str(&cfg.adapter)?;
    if adapter_needs_key(&cfg.adapter) && cfg.api_key.is_empty() {
        return Err(AiError::MissingApiKey {
            adapter: cfg.adapter.clone(),
        }
        .into());
    }
    if cfg.adapter == "openai_compatible" && cfg.base_url.is_empty() {
        return Err(AiError::MissingBaseUrl.into());
    }
    if cfg.model.trim().is_empty() {
        return Err(AiError::MissingModel {
            adapter: cfg.adapter.clone(),
        }
        .into());
    }
    Ok(())
}
