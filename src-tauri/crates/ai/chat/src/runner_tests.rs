//! run_chat 对话循环测试 — MockTurn / FixedExecutor 全走 pub trait 抽象。
//!
//! 从 `runner.rs` 内嵌测试模块外移 (源文件行数约定 ≤500), 断言语义零变化。

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use parking_lot::Mutex;
use provider::{
    AiProviderConfig, ChatMessageDto, ChatRoleDto, ChatTurnOutcome, ProviderEvent, ToolCallDto,
    ToolSpecDto,
};
use serde_json::Value;

use crate::events::{AiChatEvent, EventSink};
use crate::runner::{run_chat, ChatPayload, ChatTaskRegistry, ToolExecutor, TurnProvider};
use vofa_core::Result;

/// 脚本化回合: 每轮弹出预置产物;额外记录每轮收到的历史快照。
struct MockTurn {
    turns: Mutex<Vec<MockOutput>>,
    seen_history: Mutex<Vec<Vec<ChatMessageDto>>>,
}

enum MockOutput {
    Text(String),
    ToolCalls(Vec<ToolCallDto>),
}

fn stream_from(
    events: Vec<Result<ProviderEvent>>,
) -> Pin<Box<dyn Stream<Item = Result<ProviderEvent>> + Send>> {
    Box::pin(futures::stream::iter(events))
}

fn text_end(text: &str) -> ProviderEvent {
    ProviderEvent::TurnEnd {
        outcome: ChatTurnOutcome {
            text: text.to_string(),
            ..ChatTurnOutcome::default()
        },
        input_tokens: None,
        output_tokens: None,
    }
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
                Ok(text_end(&t)),
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
    ChatPayload {
        config: AiProviderConfig {
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
    assert!(events
        .iter()
        .any(|e| matches!(e, AiChatEvent::Delta { text } if text == "hello")));
}

/// 工具轮 → 回填 → 下一轮出最终回答;历史含 assistant 工具调用与 tool 结果。
#[tokio::test]
async fn tool_round_appends_result_then_final() {
    let provider = Arc::new(MockTurn {
        turns: Mutex::new(vec![MockOutput::ToolCalls(vec![tool_call(
            "c1", "probe", 7,
        )])]),
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
    assert!(events.iter().any(
        |e| matches!(e, AiChatEvent::ToolCall { id, name, .. } if id == "c1" && name == "probe")
    ));
    assert!(events.iter().any(
        |e| matches!(e, AiChatEvent::ToolResult { is_error: false, content, .. } if content == "42")
    ));
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
    assert!(events
        .iter()
        .any(|e| matches!(e, AiChatEvent::Error { .. })));
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
