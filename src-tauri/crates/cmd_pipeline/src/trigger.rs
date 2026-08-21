//! # 触发器 Tauri 命令
//!
//! 把 `TriggerMatcher` 暴露为前端可调用的 IPC 命令。
//!
//! - 前端 `Trigger.tsx` 在手动模式按 Fire / 自动模式 trigger 跳变时调用
//! - 返回 `{ value, matched, text, output_type }` 给前端写入 `graphOutputs` 与 `customTextOutputs`

use vofa_next_core::Result;
use vofa_next_nodes::{TriggerMatchResult, TriggerMatcher, TriggerRuleDef};

/// 执行一次触发器匹配
///
/// - `rules`: 规则列表 (已按用户配置顺序)
/// - `default_miss`: 全部未命中时 `value` 端口的默认值
/// - `default_miss_text`: 全部未命中时 `text` 端口的默认值
/// - `command`: 待匹配命令字符串 (manual 模式来自面板, auto 模式来自本地文本框)
/// - `numeric`: 可选的数值, 仅 `range` 类型规则使用; `None` 时跳过 range 规则
#[tauri::command]
pub async fn match_trigger_command(
    rules: Vec<TriggerRuleDef>,
    default_miss: f32,
    default_miss_text: String,
    command: String,
    numeric: Option<f32>,
) -> Result<TriggerMatchResult> {
    let mut matcher = TriggerMatcher::new(rules, default_miss, default_miss_text);
    Ok(matcher.match_input(&command, numeric))
}
