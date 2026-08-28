//! 对话事件契约 — 后端 → 前端经 Tauri `Channel` 推送。
//!
//! serde `tag = "type"` + snake_case,与前端 `src/types/ai.ts` 的
//! `AiChatEvent` 联合类型严格对齐。

use serde::Serialize;
use serde_json::Value;

/// 事件回调类型别名 — 与 runner 内部定义保持一致 (经 runner re-export)。
pub type EventSink = std::sync::Arc<dyn Fn(AiChatEvent) + Send + Sync>;

/// 对话过程事件流。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AiChatEvent {
    /// 助手文本增量 (逐字输出)。
    Delta {
        /// 增量文本。
        text: String,
    },
    /// 推理内容增量 (DeepSeek-R1 / Kimi thinking 等)。
    ReasoningDelta {
        /// 增量推理文本。
        text: String,
    },
    /// LLM 发起工具调用,即将执行。
    ToolCall {
        /// 调用 id (与 [`AiChatEvent::ToolResult`] 的 `id` 对应)。
        id: String,
        /// 工具名。
        name: String,
        /// JSON 参数。
        arguments: Value,
    },
    /// 工具执行完成。
    ToolResult {
        /// 对应调用的 id。
        id: String,
        /// 工具名。
        name: String,
        /// 工具输出文本 (JSON 或错误信息)。
        content: String,
        /// 是否执行失败。
        is_error: bool,
    },
    /// 对话回合正常结束 (未再发起工具调用)。
    Done {
        /// 本次消耗的总轮次 (含工具回填轮)。
        rounds: u32,
    },
    /// 任务被用户取消。
    Cancelled,
    /// 发生错误,任务终止。
    Error {
        /// 错误描述 (来自 [`vofa_core::Error`] 链)。
        message: String,
        /// 错误种类 ([`vofa_core::Error::kind()`]), 供前端本地化; 旧事件缺省为空。
        #[serde(default)]
        kind: String,
        /// 结构化字段 (adapter / model / rounds 等), 供前端本地化插值。
        #[serde(default)]
        data: Value,
    },
}
