//! 回合记录器 — 把流式事件聚合成可持久化的助手视图条目。
//!
//! 会话所有权在后端后, `ai` 在转发事件给前端的同一回调里喂给
//! [`TurnRecorder`];回合终态 (Done / Cancelled / Error) 后调用 [`TurnRecorder::finish`]
//! 得到待落盘条目。语义与前端视图聚合规则一致:
//! - 文本与工具卡片聚合为一条 assistant 条目
//! - `Error` 事件单独产出一条 error 条目 (不进 LLM 历史, 见 `session::history`)
//! - 未收到结果的工具调用在收束时标记为失败, 避免持久化"永远运行中"的卡片

use std::collections::BTreeMap;

use serde_json::Value;
use session::{ToolRunDto, ViewItemDto, ViewRoleDto};

use crate::events::AiChatEvent;

/// 终止错误 — 原始描述 + 本地化所需的结构化字段。
#[derive(Debug, Default)]
struct TurnError {
    message: String,
    kind: String,
    data: Value,
}

impl TurnError {
    fn from_event(message: &str, kind: &str, data: &Value) -> Self {
        Self {
            message: message.to_string(),
            kind: kind.to_string(),
            data: data.clone(),
        }
    }

    /// 事件 data (JSON) → 字符串映射 (仅保留字符串字段, 与前端对齐)。
    fn data_map(&self) -> Option<BTreeMap<String, String>> {
        let obj = self.data.as_object()?;
        let map: BTreeMap<String, String> = obj
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect();
        (!map.is_empty()).then_some(map)
    }
}

/// 单次助手回合的聚合器。
#[derive(Debug, Default)]
pub struct TurnRecorder {
    text: String,
    tools: Vec<ToolRunDto>,
    error: Option<TurnError>,
}

impl TurnRecorder {
    /// 新建聚合器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一条事件 (仅消费聚合相关事件, Done / Cancelled 忽略)。
    pub fn record(&mut self, event: &AiChatEvent) {
        match event {
            AiChatEvent::Delta { text } => self.text.push_str(text),
            AiChatEvent::ToolCall {
                id,
                name,
                arguments,
            } => self.tools.push(ToolRunDto {
                id: id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
                content: String::new(),
                is_error: false,
                done: false,
            }),
            AiChatEvent::ToolResult {
                id,
                content,
                is_error,
                ..
            } => {
                if let Some(run) = self.tools.iter_mut().find(|run| run.id == *id) {
                    run.content.clone_from(content);
                    run.is_error = *is_error;
                    run.done = true;
                }
            }
            AiChatEvent::Error {
                message,
                kind,
                data,
            } => {
                self.error = Some(TurnError::from_event(message, kind, data));
            }
            AiChatEvent::ReasoningDelta { .. }
            | AiChatEvent::Done { .. }
            | AiChatEvent::Cancelled => {}
        }
    }

    /// 回合收束 — 产出待持久化条目。
    ///
    /// 文本与工具均空且无错误时不产出 (空回合不落盘);
    /// 未完成的工具调用在副本上标记为失败收束。
    pub fn finish(&self) -> Vec<ViewItemDto> {
        let mut out = Vec::new();
        if !self.text.is_empty() || !self.tools.is_empty() {
            let tools: Vec<ToolRunDto> = self
                .tools
                .iter()
                .map(|run| ToolRunDto {
                    done: true,
                    is_error: run.is_error || !run.done,
                    ..run.clone()
                })
                .collect();
            out.push(ViewItemDto {
                role: ViewRoleDto::Assistant,
                text: self.text.clone(),
                tools: (!tools.is_empty()).then_some(tools),
                error: None,
                error_kind: None,
                error_data: None,
            });
        }
        if let Some(err) = &self.error {
            out.push(ViewItemDto {
                role: ViewRoleDto::Assistant,
                text: err.message.clone(),
                tools: None,
                error: Some(true),
                error_kind: (!err.kind.is_empty()).then(|| err.kind.clone()),
                error_data: err.data_map(),
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool_call(id: &str) -> AiChatEvent {
        AiChatEvent::ToolCall {
            id: id.to_string(),
            name: "probe".to_string(),
            arguments: json!({"value": 7}),
        }
    }

    fn tool_result(id: &str, is_error: bool) -> AiChatEvent {
        AiChatEvent::ToolResult {
            id: id.to_string(),
            name: "probe".to_string(),
            content: "42".to_string(),
            is_error,
        }
    }

    #[test]
    fn aggregates_text_and_tools_into_one_item() {
        let mut rec = TurnRecorder::new();
        rec.record(&AiChatEvent::Delta {
            text: "你好".into(),
        });
        rec.record(&tool_call("c1"));
        rec.record(&tool_result("c1", false));
        rec.record(&AiChatEvent::Done { rounds: 2 });

        let items = rec.finish();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].role, ViewRoleDto::Assistant);
        assert_eq!(items[0].text, "你好");
        let tools = items[0].tools.as_ref().unwrap();
        assert_eq!(tools.len(), 1);
        assert!(tools[0].done);
        assert!(!tools[0].is_error);
        assert_eq!(tools[0].content, "42");
    }

    /// Error 事件产出独立 error 条目 (排在回合条目之后, 与前端一致),
    /// kind / 结构化字段随条目持久化供前端本地化。
    #[test]
    fn error_event_produces_error_item() {
        let mut rec = TurnRecorder::new();
        rec.record(&AiChatEvent::Delta {
            text: "part".into(),
        });
        rec.record(&AiChatEvent::Error {
            message: "LLM 请求失败 [orcarouter/openai/gpt-4o-mini]: 401".into(),
            kind: "AiProviderRequest".into(),
            data: serde_json::json!({"adapter": "orcarouter", "model": "openai/gpt-4o-mini"}),
        });

        let items = rec.finish();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "part");
        assert_eq!(
            items[1].text,
            "LLM 请求失败 [orcarouter/openai/gpt-4o-mini]: 401"
        );
        assert_eq!(items[1].error, Some(true));
        assert_eq!(items[1].error_kind.as_deref(), Some("AiProviderRequest"));
        let data = items[1].error_data.as_ref().unwrap();
        assert_eq!(data.get("adapter").map(String::as_str), Some("orcarouter"));
        assert_eq!(
            data.get("model").map(String::as_str),
            Some("openai/gpt-4o-mini")
        );
    }

    /// 缺少 kind 的旧事件 → error_kind 不落盘 (前端回退显示原始描述)。
    #[test]
    fn error_without_kind_keeps_raw_message_only() {
        let mut rec = TurnRecorder::new();
        rec.record(&AiChatEvent::Error {
            message: "boom".into(),
            kind: String::new(),
            data: Value::Null,
        });

        let items = rec.finish();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "boom");
        assert!(items[0].error_kind.is_none());
        assert!(items[0].error_data.is_none());
    }

    /// 取消时未完成的工具调用收束为失败, 不残留"运行中"状态。
    #[test]
    fn unfinished_tools_finalized_as_failed() {
        let mut rec = TurnRecorder::new();
        rec.record(&AiChatEvent::Delta { text: "查".into() });
        rec.record(&tool_call("c1"));
        rec.record(&AiChatEvent::Cancelled);

        let items = rec.finish();
        assert_eq!(items.len(), 1);
        let tools = items[0].tools.as_ref().unwrap();
        assert!(tools[0].done);
        assert!(tools[0].is_error);
    }

    /// 空回合不产出条目。
    #[test]
    fn empty_turn_produces_nothing() {
        let mut rec = TurnRecorder::new();
        rec.record(&AiChatEvent::Done { rounds: 1 });
        assert!(rec.finish().is_empty());
    }
}
