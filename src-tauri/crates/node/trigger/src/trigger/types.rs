//! 触发器公共类型 — 与前端 `TriggerRule` / `TriggerMatchResult` 对齐

use serde::{Deserialize, Serialize};

/// 匹配类型 — 与前端 `TriggerMatchType` 对齐
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TriggerMatchType {
    Exact,
    Prefix,
    Contains,
    Regex,
    Range,
    Glob,
}

/// 单条匹配规则 — 与前端 `TriggerRule` 对齐
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct TriggerRuleDef {
    pub id: String,
    pub pattern: String,
    pub match_type: TriggerMatchType,
    #[serde(default)]
    pub flags: Option<String>,
    /// 输出值类型: 'number' | 'string' (默认 'number', 兼容旧配置)
    #[serde(default = "default_output_type")]
    pub output_type: String,
    /// 数字输出值 (output_type='number' 时使用)
    #[serde(default)]
    pub output_value: f32,
    /// 字符串输出值 (output_type='string' 时使用)
    #[serde(default)]
    pub output_text: String,
    pub enabled: bool,
}

fn default_output_type() -> String {
    "number".to_string()
}

/// 匹配结果 — 返回前端 (camelCase, 对齐前端 TriggerMatchResult 类型)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerMatchResult {
    pub value: f32,
    pub matched: bool,
    /// 字符串输出 (output_type='string' 时填充规则 output_text, 否则 default_miss_text)
    pub text: String,
    /// 本次匹配的输出类型: 'number' | 'string' | 'miss'
    pub output_type: String,
}
