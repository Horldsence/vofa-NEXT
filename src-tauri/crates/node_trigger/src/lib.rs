//! # node_trigger
//!
//! VOFA-NEXT 触发器匹配器 — 镜像前端 `TriggerRule` 配置, 在后端实现
//! 命令字符串 → 数值/字符串对照表查找。
//!
//! 支持的匹配类型 (与前端 `TriggerMatchType` 对齐):
//! - `Exact`:    字符串完全相等
//! - `Prefix`:   命令以模式开头
//! - `Contains`: 命令包含模式
//! - `Regex`:    JavaScript/PCRE 风格正则 (Rust `regex` crate)
//! - `Range`:    命令解析为 f64, 落在 `[min..max]` 内 (支持 `Infinity` / `-Infinity`)
//! - `Glob`:     标准 shell glob 模式 (`*` / `?` / `[abc]` / `{a,b,c}`)
//!
//! 输出值类型: 每条规则 `output_type: 'number' | 'string'`,
//! 命中时分别填充 `output_value: f32` 或 `output_text: String`。
//!
//! 规则按顺序求值, 首个命中规则即返回;
//! 全部未命中则返回 `{ value: default_miss, text: default_miss_text, matched: false }`。
//! 正则 / glob 按 `rule.id` 缓存, 同一规则多次匹配复用编译结果。

mod trigger;

pub use trigger::{
    TriggerMatchResult, TriggerMatchType, TriggerMatcher, TriggerRuleDef, TriggerState,
    format_auto_command, parse_range,
};
