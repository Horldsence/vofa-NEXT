//! 对话循环执行器 — 多轮工具调用与任务取消注册表。

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

use ai_provider::{
    AiProviderConfig, ChatMessageDto, ChatRoleDto, ChatTurnOutcome, ProviderEvent, ToolCallDto,
    ToolSpecDto, chat_turn_stream, validate_config,
};
use async_trait::async_trait;
use error::{AiError, Error as _};
use futures::{Stream, StreamExt};
use parking_lot::Mutex;
use serde_json::Value;
use tokio::sync::watch;
use vofa_core::Result;

use crate::events::EventSink;
use crate::events::AiChatEvent;


/// 单次对话请求载荷。
#[derive(Debug, Clone)]
pub struct ChatPayload {
    /// LLM provider 配置 (含 key,不落盘)。
    pub config: AiProviderConfig,
    /// 系统提示词 (可选)。
    pub system: Option<String>,
    /// 完整对话历史 (前端持有,含上一轮工具调用与回填)。
    pub messages: Vec<ChatMessageDto>,
    /// 工具循环最大轮次保护。
    pub max_tool_rounds: u32,
}

/// LLM 单轮流式回合的抽象 — 生产实现走 genai,测试可 mock。
#[async_trait]
pub trait TurnProvider: Send + Sync {
    /// 执行一轮流式对话,返回增量事件流。
    ///
    /// # Errors
    /// 请求发起失败 (网络 / 鉴权 / 配置) 时返回错误。
    async fn chat_turn(
        &self,
        cfg: &AiProviderConfig,
        system: Option<String>,
        messages: &[ChatMessageDto],
        tools: &[ToolSpecDto],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ProviderEvent>> + Send>>>;
}

/// 工具执行器抽象 — 生产实现由 MCP client 聚合工具,测试可 mock。
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// 当前可用工具清单 (每轮调用,允许动态变化)。
    fn tools(&self) -> Vec<ToolSpecDto>;

    /// 执行工具;`Err` 视为工具失败 (is_error 回填给 LLM)。
    ///
    /// # Errors
    /// 工具不存在或执行失败时返回错误。
    async fn call(&self, name: &str, arguments: Value) -> Result<String>;
}

/// genai 生产实现。
pub struct GenaiTurnProvider;

#[async_trait]
impl TurnProvider for GenaiTurnProvider {
    async fn chat_turn(
        &self,
        cfg: &AiProviderConfig,
        system: Option<String>,
        messages: &[ChatMessageDto],
        tools: &[ToolSpecDto],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ProviderEvent>> + Send>>> {
        let stream = chat_turn_stream(cfg, system, messages, tools).await?;
        Ok(Box::pin(stream))
    }
}

/// 对话任务取消注册表 — task_id → 取消标志发送端。
#[derive(Default)]
pub struct ChatTaskRegistry {
    tasks: Mutex<HashMap<String, watch::Sender<bool>>>,
    seq: AtomicU64,
}

impl ChatTaskRegistry {
    /// 生成新 task_id 并登记取消通道;返回 (task_id, 接收端)。
    pub fn register(&self) -> (String, watch::Receiver<bool>) {
        let id = format!(
            "chat-{:x}-{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
            self.seq.fetch_add(1, Ordering::Relaxed)
        );
        let (tx, rx) = watch::channel(false);
        self.tasks.lock().insert(id.clone(), tx);
        (id, rx)
    }

    /// 取消任务;返回是否存在该任务。
    pub fn cancel(&self, task_id: &str) -> bool {
        self.tasks
            .lock()
            .get(task_id)
            .is_some_and(|tx| tx.send_if_modified(|flag| {
                if *flag {
                    false
                } else {
                    *flag = true;
                    true
                }
            }))
    }

    /// 任务结束后移除登记。
    pub fn remove(&self, task_id: &str) {
        self.tasks.lock().remove(task_id);
    }
}

/// 助手回合产物 → 历史 assistant 消息 (文本 + 工具调用)。
fn assistant_message(outcome: &ChatTurnOutcome) -> ChatMessageDto {
    ChatMessageDto {
        role: ChatRoleDto::Assistant,
        content: outcome.text.clone(),
        tool_calls: (!outcome.tool_calls.is_empty()).then(|| outcome.tool_calls.clone()),
        tool_call_id: None,
        name: None,
    }
}

/// 工具执行结果 → 历史 tool 消息。
fn tool_message(call: &ToolCallDto, content: String) -> ChatMessageDto {
    ChatMessageDto {
        role: ChatRoleDto::Tool,
        content,
        tool_calls: None,
        tool_call_id: Some(call.id.clone()),
        name: Some(call.name.clone()),
    }
}

/// `vofa_core::Error` → 终止事件 — kind 与结构化字段随事件下发, 供前端本地化。
fn error_event(e: &vofa_core::Error) -> AiChatEvent {
    AiChatEvent::Error {
        message: e.to_string(),
        kind: e.kind().to_string(),
        data: serde_json::to_value(e.data_fields()).unwrap_or_default(),
    }
}

/// 收集一轮流式事件:增量实时回调,返回聚合产物。
///
/// 取消标志置位时立即返回 [`AiError::Cancelled`]。
async fn collect_turn(
    stream: Pin<Box<dyn Stream<Item = Result<ProviderEvent>> + Send>>,
    cancel: &mut watch::Receiver<bool>,
    on_event: &EventSink,
) -> Result<ChatTurnOutcome> {
    let mut stream = stream;
    let mut outcome = ChatTurnOutcome::default();
    loop {
        tokio::select! {
            biased;
            res = cancel.changed() => {
                // 发送端被移除等价于任务已结束, 不视为取消
                if res.is_ok() && *cancel.borrow() {
                    on_event(AiChatEvent::Cancelled);
                    return Err(AiError::Cancelled.into());
                }
            }
            item = stream.next() => {
                let Some(item) = item else { break; };
                match item {
                    Ok(ProviderEvent::TextDelta(text)) => {
                        outcome.text.push_str(&text);
                        on_event(AiChatEvent::Delta { text });
                    }
                    Ok(ProviderEvent::ReasoningDelta(text)) => {
                        outcome.reasoning.push_str(&text);
                        on_event(AiChatEvent::ReasoningDelta { text });
                    }
                    Ok(ProviderEvent::TurnEnd { outcome: end, .. }) => {
                        // 以流末捕获的聚合值为准 (完整且按序)
                        outcome = end;
                    }
            Err(e) => {
                on_event(error_event(&e));
                return Err(e);
            }
                }
            }
        }
    }
    Ok(outcome)
}

/// 执行一次完整对话 (可含多轮工具调用),事件实时回调。
///
/// 正常结束 (本轮无工具调用) 返回 `Ok(rounds)`;工具循环耗尽返回
/// [`AiError::MaxToolRounds`];取消返回 [`AiError::Cancelled`] —
/// 取消与循环耗尽已先回调对应事件。
///
/// # Errors
/// 见 [`AiError`] 各变体。
pub async fn run_chat(
    payload: ChatPayload,
    provider: &dyn TurnProvider,
    executor: &dyn ToolExecutor,
    mut cancel: watch::Receiver<bool>,
    on_event: EventSink,
) -> Result<u32> {
    validate_config(&payload.config)?;

    let max_rounds = payload.max_tool_rounds.max(1);
    let mut history = payload.messages;
    let mut rounds: u32 = 0;

    for _ in 0..max_rounds {
        rounds += 1;
        let stream = provider
            .chat_turn(
                &payload.config,
                payload.system.clone(),
                &history,
                &executor.tools(),
            )
            .await?;
        let outcome = collect_turn(stream, &mut cancel, &on_event).await?;

        history.push(assistant_message(&outcome));
        if outcome.tool_calls.is_empty() {
            on_event(AiChatEvent::Done { rounds });
            return Ok(rounds);
        }

        // 逐个执行工具并回填 (LLM 可能在单轮发起多个调用)
        for call in outcome.tool_calls {
            if *cancel.borrow() {
                on_event(AiChatEvent::Cancelled);
                return Err(AiError::Cancelled.into());
            }
            on_event(AiChatEvent::ToolCall {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            });
            let result = executor.call(&call.name, call.arguments.clone()).await;
            match result {
                Ok(content) => {
                    on_event(AiChatEvent::ToolResult {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        is_error: false,
                        content: content.clone(),
                    });
                    history.push(tool_message(&call, content));
                }
                Err(e) => {
                    let message = e.to_string();
                    on_event(AiChatEvent::ToolResult {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        is_error: true,
                        content: message.clone(),
                    });
                    // 失败结果同样回填, 让 LLM 有机会自我修正
                    history.push(tool_message(&call, message));
                }
            }
        }
    }

    let err = AiError::MaxToolRounds { rounds }.into();
    on_event(error_event(&err));
    Err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// 脚本化回合: 每轮弹出预置产物;额外记录每轮收到的历史快照。
    struct MockTurn {
        turns: Mutex<Vec<MockOutput>>,
        seen_history: Mutex<Vec<Vec<ChatMessageDto>>>,
    }

    enum MockOutput {
        Text(String),
        ToolCalls(Vec<ToolCallDto>),
    }

    fn stream_from(events: Vec<Result<ProviderEvent>>) -> Pin<Box<dyn Stream<Item = Result<ProviderEvent>> + Send>> {
        Box::pin(futures::stream::iter(events))
    }

    fn text_end(text: &str) -> Result<ProviderEvent> {
        Ok(ProviderEvent::TurnEnd {
            outcome: ChatTurnOutcome {
                text: text.to_string(),
                ..ChatTurnOutcome::default()
            },
            input_tokens: None,
            output_tokens: None,
        })
    }

    fn tool_call(id: &str, name: &str, value: i64) -> ToolCallDto {
        ToolCallDto {
            id: id.to_string(),
            name: name.to_string(),
            arguments: serde_json::json!({ "value": value }),
        }
    }

    #[async_trait]
    impl TurnProvider for MockTurn {
        async fn chat_turn(
            &self,
            _cfg: &AiProviderConfig,
            _system: Option<String>,
            messages: &[ChatMessageDto],
            _tools: &[ToolSpecDto],
        ) -> Result<Pin<Box<dyn Stream<Item = Result<ProviderEvent>> + Send>>> {
            self.seen_history.lock().push(messages.to_vec());
            let mut turns = self.turns.lock();
            let output = if turns.is_empty() {
                MockOutput::Text("done".to_string())
            } else {
                turns.remove(0)
            };
            Ok(match output {
                MockOutput::Text(t) => stream_from(vec![
                    Ok(ProviderEvent::TextDelta(t.clone())),
                    text_end(&t),
                ]),
                MockOutput::ToolCalls(calls) => stream_from(vec![Ok(ProviderEvent::TurnEnd {
                    outcome: ChatTurnOutcome {
                        tool_calls: calls,
                        ..ChatTurnOutcome::default()
                    },
                    input_tokens: None,
                    output_tokens: None,
                })]),
            })
        }
    }

    struct FixedExecutor;

    #[async_trait]
    impl ToolExecutor for FixedExecutor {
        fn tools(&self) -> Vec<ToolSpecDto> {
            vec![ToolSpecDto {
                name: "probe".to_string(),
                description: "test tool".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
            }]
        }

        async fn call(&self, name: &str, _arguments: Value) -> Result<String> {
            assert_eq!(name, "probe");
            Ok("42".to_string())
        }
    }

    fn payload(max_rounds: u32) -> ChatPayload {
        ChatPayload {            config: AiProviderConfig {
                adapter: "openai".to_string(),
                base_url: String::new(),
                api_key: "sk-test".to_string(),
                model: "gpt-4o-mini".to_string(),
                temperature: None,
                max_tokens: None,
            },
            system: None,
            messages: vec![ChatMessageDto {
                role: ChatRoleDto::User,
                content: "hi".to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            max_tool_rounds: max_rounds,
        }
    }

    fn sink_into(events: &Arc<Mutex<Vec<AiChatEvent>>>) -> EventSink {
        let sink = Arc::clone(events);
        Arc::new(move |e| sink.lock().push(e))
    }

    /// 纯文本单轮: 直接 Done, 不进工具循环。
    #[tokio::test]
    async fn text_only_completes_in_one_round() {
        let provider = MockTurn {
            turns: Mutex::new(vec![MockOutput::Text("hello".to_string())]),
            seen_history: Mutex::new(vec![]),
        };
        let events = Arc::new(Mutex::new(vec![]));
        let registry = ChatTaskRegistry::default();
        let (_id, cancel_rx) = registry.register();

        let rounds = run_chat(
            payload(4),
            &provider,
            &FixedExecutor,
            cancel_rx,
            sink_into(&events),
        )
        .await
        .expect("纯文本应一轮完成");
        assert_eq!(rounds, 1);

        let events = events.lock();
        assert!(events
            .iter()
            .any(|e| matches!(e, AiChatEvent::Done { rounds: 1 })));
        assert!(events.iter().any(|e| matches!(e, AiChatEvent::Delta { text } if text == "hello")));
    }

    /// 工具轮 → 回填 → 下一轮出最终回答;历史含 assistant 工具调用与 tool 结果。
    #[tokio::test]
    async fn tool_round_appends_result_then_final() {
        let provider = Arc::new(MockTurn {
            turns: Mutex::new(vec![MockOutput::ToolCalls(vec![tool_call("c1", "probe", 7)])]),
            seen_history: Mutex::new(vec![]),
        });
        let events = Arc::new(Mutex::new(vec![]));
        let registry = ChatTaskRegistry::default();
        let (_id, cancel_rx) = registry.register();

        let rounds = run_chat(
            payload(4),
            &*provider,
            &FixedExecutor,
            cancel_rx,
            sink_into(&events),
        )
        .await
        .expect("工具循环应两轮完成");
        assert_eq!(rounds, 2);

        // 第二轮收到的历史: user + assistant(tool_calls) + tool(result)
        let seen = provider.seen_history.lock();
        assert_eq!(seen.len(), 2);
        let second = &seen[1];
        assert_eq!(second.len(), 3);
        assert_eq!(second[1].role, ChatRoleDto::Assistant);
        assert_eq!(second[1].tool_calls.as_ref().map(Vec::len), Some(1));
        assert_eq!(second[2].role, ChatRoleDto::Tool);
        assert_eq!(second[2].content, "42");
        assert_eq!(second[2].tool_call_id.as_deref(), Some("c1"));

        let events = events.lock();
        assert!(events.iter().any(|e| matches!(e, AiChatEvent::ToolCall { id, name, .. } if id == "c1" && name == "probe")));
        assert!(events
            .iter()
            .any(|e| matches!(e, AiChatEvent::ToolResult { is_error: false, content, .. } if content == "42")));
    }

    /// 工具循环每轮都发起调用 → 超出 max_tool_rounds 报错并回调 Error。
    #[tokio::test]
    async fn exceeds_max_rounds_errors() {
        // 两轮各自发起一次调用, 脚本耗尽不回退文本
        let always_tools = MockTurn {
            turns: Mutex::new(vec![
                MockOutput::ToolCalls(vec![tool_call("c1", "probe", 1)]),
                MockOutput::ToolCalls(vec![tool_call("c2", "probe", 2)]),
            ]),
            seen_history: Mutex::new(vec![]),
        };
        let events = Arc::new(Mutex::new(vec![]));
        let registry = ChatTaskRegistry::default();
        let (_id, cancel_rx) = registry.register();

        let result = run_chat(
            payload(2),
            &always_tools,
            &FixedExecutor,
            cancel_rx,
            sink_into(&events),
        )
        .await;
        assert!(result.is_err(), "超轮次应返回错误");
        let events = events.lock();
        assert!(events.iter().any(|e| matches!(e, AiChatEvent::Error { .. })));
    }

    /// 取消标志置位 → Cancelled 错误 + Cancelled 事件。
    #[tokio::test]
    async fn cancel_flag_aborts_turn() {
        let provider = MockTurn {
            turns: Mutex::new(vec![MockOutput::Text("never".to_string())]),
            seen_history: Mutex::new(vec![]),
        };
        let events = Arc::new(Mutex::new(vec![]));
        let registry = ChatTaskRegistry::default();
        let (id, cancel_rx) = registry.register();
        assert!(registry.cancel(&id), "取消已登记任务应成功");

        let result = run_chat(
            payload(4),
            &provider,
            &FixedExecutor,
            cancel_rx,
            sink_into(&events),
        )
        .await;
        assert!(matches!(
            result,
            Err(vofa_core::Error::Ai(error::AiError::Cancelled))
        ));
        let events = events.lock();
        assert!(events.iter().any(|e| matches!(e, AiChatEvent::Cancelled)));
    }
}
