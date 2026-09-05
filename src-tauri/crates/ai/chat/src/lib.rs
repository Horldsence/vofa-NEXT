//! # chat
//!
//! AI 对话编排 — 多轮工具调用 (agentic loop) 与任务取消。
//!
//! 核心循环 ([`runner::run_chat`]):
//! 1. 带完整历史调用 LLM (流式,增量事件实时回调)
//! 2. 若本轮产生工具调用 → 逐个执行 (`ToolExecutor`,由 MCP 聚合实现) →
//!    结果以 tool 消息回填历史 → 进入下一轮
//! 3. 无工具调用 → 本轮文本即最终回答,结束
//!
//! 循环轮次受 `max_tool_rounds` 保护;任意时刻可通过 `watch` 取消标志中断。
//! LLM 调用经 [`runner::TurnProvider`] 抽象,工具执行经
//! [`runner::ToolExecutor`] 抽象,二者均可 mock,循环逻辑可离线单测。

pub mod events;
pub mod recorder;
pub mod runner;

#[cfg(test)]
mod runner_tests;

pub use events::{AiChatEvent, EventSink};
pub use recorder::TurnRecorder;
pub use runner::{
    run_chat, ChatPayload, ChatTaskRegistry, GenaiTurnProvider, ToolExecutor, TurnProvider,
};
