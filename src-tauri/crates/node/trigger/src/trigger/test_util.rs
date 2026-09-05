//! 测试共用构造器 — matcher / state 测试模块共用

use super::types::{TriggerMatchType, TriggerRuleDef};

/// 数值断言统一入口 — 输出值由字面规则/缺省值精确给出, 单点放宽浮点严格相等
#[allow(clippy::float_cmp)]
pub fn assert_value(actual: f32, expected: f32) {
    assert_eq!(actual, expected);
}

pub fn rule(
    id: &str,
    mt: TriggerMatchType,
    pattern: &str,
    value: f32,
    enabled: bool,
) -> TriggerRuleDef {
    TriggerRuleDef {
        id: id.to_string(),
        pattern: pattern.to_string(),
        match_type: mt,
        flags: None,
        output_type: "number".to_string(),
        output_value: value,
        output_text: String::new(),
        enabled,
    }
}

pub fn string_rule(
    id: &str,
    mt: TriggerMatchType,
    pattern: &str,
    text: &str,
    enabled: bool,
) -> TriggerRuleDef {
    TriggerRuleDef {
        id: id.to_string(),
        pattern: pattern.to_string(),
        match_type: mt,
        flags: None,
        output_type: "string".to_string(),
        output_value: 0.0,
        output_text: text.to_string(),
        enabled,
    }
}
