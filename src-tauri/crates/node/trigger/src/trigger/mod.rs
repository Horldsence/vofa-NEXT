//! # 触发器匹配器 (Trigger)
//!
//! 镜像前端 `TriggerRule` 配置, 在后端实现命令字符串 → 数值/字符串对照表查找。
//!
//! 匹配方法:
//! - `Exact`:    字符串完全相等
//! - `Prefix`:   命令以模式开头
//! - `Contains`: 命令包含模式
//! - `Regex`:    JavaScript/PCRE 风格正则 (Rust `regex` crate)
//! - `Range`:    命令解析为 f64, 落在 `[min..max]` 内 (支持 `Infinity` / `-Infinity`)
//! - `Glob`:     标准 shell glob 模式 (`*` / `?` / `[abc]` / `{a,b,c}`, Rust `glob` crate)
//!
//! 输出值类型: 每条规则 `output_type: 'number' | 'string'`,
//! 命中时分别填充 `output_value: f32` 或 `output_text: String`。
//!
//! 规则按顺序求值, 首个命中规则即返回;
//! 全部未命中则返回 `{ value: default_miss, text: default_miss_text, matched: false }`。
//!
//! 正则 / glob 按 `rule.id` 缓存, 同一规则多次匹配复用编译结果。

mod matcher;
mod pattern;
mod state;
mod types;

pub use matcher::TriggerMatcher;
pub use pattern::parse_range;
pub use state::{format_auto_command, TriggerState};
pub use types::{TriggerMatchResult, TriggerMatchType, TriggerRuleDef};

#[cfg(test)]
mod test_util;
