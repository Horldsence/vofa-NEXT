//! 视图条目流 → LLM 消息历史派生。
//!
//! 历史的所有权在后端:会话以视图条目形式落盘, 发起对话时由此模块
//! 派生 LLM 所需的消息序列 (与原前端 `exchangeToHistory` 语义一致, 并修正
//! 一处配对问题 — 未完成的工具调用从 `tool_calls` 中整体剔除, 保证
//! assistant.tool_calls 与 tool 结果消息一一对应)。

use ai_provider::{ChatMessageDto, ChatRoleDto, ToolCallDto};

use crate::types::{ViewItemDto, ViewRoleDto};

/// 从视图条目派生完整对话历史。
///
/// 规则:
/// - user 条目 → user 消息
/// - assistant 条目 → assistant 消息 (带已完成工具的 tool_calls) + 逐个 tool 结果
/// - `error` 条目仅用于 UI 展示, 不入历史
/// - 未收到结果的工具调用 (`done == false`) 连同结果一起剔除, 避免悬空调用
pub fn derive_history(items: &[ViewItemDto]) -> Vec<ChatMessageDto> {
    let mut out = Vec::new();
    for item in items {
        match item.role {
            ViewRoleDto::User => out.push(ChatMessageDto {
                role: ChatRoleDto::User,
                content: item.text.clone(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }),
            ViewRoleDto::Assistant => {
                if item.error == Some(true) {
                    continue;
                }
                let runs = item.tools.as_deref().unwrap_or_default();
                let finished: Vec<&crate::types::ToolRunDto> =
                    runs.iter().take_while(|run| run.done).collect();
                let calls: Vec<ToolCallDto> = finished
                    .iter()
                    .map(|run| ToolCallDto {
                        id: run.id.clone(),
                        name: run.name.clone(),
                        arguments: run.arguments.clone(),
                    })
                    .collect();
                out.push(ChatMessageDto {
                    role: ChatRoleDto::Assistant,
                    content: item.text.clone(),
                    tool_calls: (!calls.is_empty()).then_some(calls),
                    tool_call_id: None,
                    name: None,
                });
                for run in finished {
                    out.push(ChatMessageDto {
                        role: ChatRoleDto::Tool,
                        content: run.content.clone(),
                        tool_calls: None,
                        tool_call_id: Some(run.id.clone()),
                        name: Some(run.name.clone()),
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolRunDto;
    use serde_json::json;

    fn user(text: &str) -> ViewItemDto {
        ViewItemDto {
            role: ViewRoleDto::User,
            text: text.to_string(),
            tools: None,
            error: None,
            error_kind: None,
            error_data: None,
        }
    }

    fn run(id: &str, done: bool) -> ToolRunDto {
        ToolRunDto {
            id: id.to_string(),
            name: "probe".to_string(),
            arguments: json!({}),
            content: if done { "42".to_string() } else { String::new() },
            is_error: false,
            done,
        }
    }

    #[test]
    fn plain_exchange_maps_two_messages() {
        let items = vec![
            user("你好"),
            ViewItemDto {
                role: ViewRoleDto::Assistant,
                text: "你好!".to_string(),
                tools: None,
                error: None,
                error_kind: None,
                error_data: None,
            },
        ];
        let history = derive_history(&items);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, ChatRoleDto::User);
        assert_eq!(history[0].content, "你好");
        assert_eq!(history[1].role, ChatRoleDto::Assistant);
        assert!(history[1].tool_calls.is_none());
    }

    #[test]
    fn finished_tool_calls_pair_with_results() {
        let items = vec![
            user("查一下"),
            ViewItemDto {
                role: ViewRoleDto::Assistant,
                text: String::new(),
                tools: Some(vec![run("c1", true)]),
                error: None,
                error_kind: None,
                error_data: None,
            },
            ViewItemDto {
                role: ViewRoleDto::Assistant,
                text: "结果是 42".to_string(),
                tools: None,
                error: None,
                error_kind: None,
                error_data: None,
            },
        ];
        let history = derive_history(&items);
        assert_eq!(history.len(), 4);
        assert_eq!(history[1].role, ChatRoleDto::Assistant);
        assert_eq!(history[1].tool_calls.as_ref().map(Vec::len), Some(1));
        assert_eq!(history[2].role, ChatRoleDto::Tool);
        assert_eq!(history[2].tool_call_id.as_deref(), Some("c1"));
        assert_eq!(history[2].content, "42");
        assert_eq!(history[3].content, "结果是 42");
    }

    /// 未完成的工具调用整体剔除 (不产生悬空 tool_calls);
    /// error 条目不进历史。
    #[test]
    fn unfinished_tools_and_error_items_are_skipped() {
        let items = vec![
            user("hi"),
            ViewItemDto {
                role: ViewRoleDto::Assistant,
                text: "partial".to_string(),
                tools: Some(vec![run("c1", false)]),
                error: None,
                error_kind: None,
                error_data: None,
            },
            ViewItemDto {
                role: ViewRoleDto::Assistant,
                text: "请求失败".to_string(),
                tools: None,
                error: Some(true),
                error_kind: Some("AiProviderRequest".to_string()),
                error_data: None,
            },
        ];
        let history = derive_history(&items);
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].role, ChatRoleDto::Assistant);
        assert_eq!(history[1].content, "partial");
        assert!(history[1].tool_calls.is_none());
    }
}
