//! Trigger 节点求值状态 — 跨帧持久 (边沿检测 prev 值 + 匹配器)

use super::matcher::TriggerMatcher;
use super::types::{TriggerMatchResult, TriggerRuleDef};

/// 命令字符串 → Range 匹配数值 (对齐前端 `Number(trimmed)` + `Number.isFinite` 判定):
/// 空串 / 解析失败 / 非有限值 (NaN/±Infinity) → `None` (跳过 Range 规则)
fn numeric_of(command: &str) -> Option<f32> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// 自动模式匹配输入的命令字符串 — 对齐前端 `String(triggerValue)` (JS Number→String)
///
/// Rust `Display` 已覆盖常见情形 (整数不带 `.0`, 最短回 round-trip 小数);
/// 这里补齐 JS 语义差异: `-0` → `"0"`, NaN → `"NaN"`, ±∞ → `"Infinity"/"-Infinity"`。
/// 已知偏差: JS 对 |v| ≥ 1e21 或 < 1e-6 用指数记法 (`"1e+21"`), Rust `Display` 不用 —
/// 极端量级下 exact/prefix 等字符串规则的匹配结果可能与前端不一致。
#[must_use]
pub fn format_auto_command(v: f32) -> String {
    if v == 0.0 {
        "0".to_string()
    } else if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
    } else {
        format!("{v}")
    }
}

/// Trigger 节点求值状态 — 跨帧持久 (regex/glob 缓存 + 边沿检测 prev 值)
///
/// 生命周期仿 `filter_states` (DigitalFilter): 图求值时懒建,
/// 匹配器相关配置 (rules/default_miss/default_miss_text) 变化时重建,
/// 节点删除时由调用方清理。
/// mode/edge/command 不参与重建比较 — 求值时现读 (对齐前端: 这些变更不重置 prevTriggerRef,
/// 但规则/default 变更会导致重建并复位 prev_trigger, 与 Filter kind 变更重建同语义)。
pub struct TriggerState {
    matcher: TriggerMatcher,
    /// 重建依据快照 (仅匹配器相关配置)
    rules: Vec<TriggerRuleDef>,
    default_miss: f32,
    default_miss_text: String,
    /// 自动模式边沿检测: 上一帧 trigger 输入值 (对齐前端 prevTriggerRef, 初始 0)
    prev_trigger: f32,
}

impl TriggerState {
    #[must_use]
    pub fn new(rules: Vec<TriggerRuleDef>, default_miss: f32, default_miss_text: String) -> Self {
        Self {
            matcher: TriggerMatcher::new(rules.clone(), default_miss, default_miss_text.clone()),
            rules,
            default_miss,
            default_miss_text,
            prev_trigger: 0.0,
        }
    }

    /// 匹配器相关配置是否一致 (不一致 → 调用方重建, 同 Filter 的 kind 变化重建)
    #[allow(clippy::float_cmp)] // 配置同一性比较 (按位相等即重建), 非数值逼近
    pub fn matches_config(
        &self,
        rules: &[TriggerRuleDef],
        default_miss: f32,
        default_miss_text: &str,
    ) -> bool {
        self.rules == rules
            && self.default_miss == default_miss
            && self.default_miss_text == default_miss_text
    }

    /// 手动模式求值: 以 command 直接匹配 (前端 Fire 按钮的 runMatch 等价物)
    pub fn eval_manual(&mut self, command: &str) -> TriggerMatchResult {
        self.matcher.match_input(command, numeric_of(command))
    }

    /// 非 auto 模式下的 prev 跟踪 — 对齐前端 useEffect
    /// (`mode !== 'auto'` 时仍每帧 `prevTriggerRef.current = triggerValue`):
    /// 保证 manual 期间输入 0→正 后切回 auto+rising 不会因陈旧 prev 误触发上升沿
    pub const fn record_prev(&mut self, trigger_value: f32) {
        self.prev_trigger = trigger_value;
    }

    /// 自动模式求值: 边沿检测 + 命中时匹配 (对齐前端 Trigger.tsx 的 useEffect)
    ///
    /// - `edge == "rising"`: 仅 `prev == 0 && value > 0` 的上升沿匹配一次
    /// - 其他 (`"level"`): `value != 0` 期间每帧匹配
    /// - 未激活返回 `None` (本帧不更新输出, 端口保持上次值); `prev_trigger` 每帧更新
    ///
    /// 匹配输入对齐前端 `runMatch(String(triggerValue))`:
    /// 命令为数值的字符串形式, Range 数值为该值本身 (非有限 → None)
    pub fn eval_auto(&mut self, edge: &str, trigger_value: f32) -> Option<TriggerMatchResult> {
        let prev = self.prev_trigger;
        self.prev_trigger = trigger_value;
        let active = if edge == "rising" {
            prev == 0.0 && trigger_value > 0.0
        } else {
            trigger_value != 0.0
        };
        if !active {
            return None;
        }
        let cmd = format_auto_command(trigger_value);
        let numeric = trigger_value.is_finite().then_some(trigger_value);
        Some(self.matcher.match_input(&cmd, numeric))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::rule;
    use super::super::types::TriggerMatchType;
    use super::*;

    fn state_with_range_rule() -> TriggerState {
        // Range 规则: trigger 值落在 1..10 → 输出 7.0 (auto 模式用数值字符串作命令)
        TriggerState::new(
            vec![rule("r1", TriggerMatchType::Range, "1..10", 7.0, true)],
            0.0,
            String::new(),
        )
    }

    #[test]
    fn edge_level_fires_every_frame_while_nonzero() {
        let mut st = state_with_range_rule();
        assert!(st.eval_auto("level", 0.0).is_none()); // 0 → 不激活
        assert!(st.eval_auto("level", 5.0).is_some());
        assert!(st.eval_auto("level", 5.0).is_some()); // 持续非零 → 每帧都匹配
        assert!(st.eval_auto("level", -1.0).is_some()); // 负值也激活 (!= 0)
        assert!(st.eval_auto("level", 0.0).is_none());
    }

    #[test]
    fn edge_rising_fires_once_then_rearms() {
        let mut st = state_with_range_rule();
        // 上升沿: 0 → 5 触发一次
        assert!(st.eval_auto("rising", 0.0).is_none());
        let r = st.eval_auto("rising", 5.0).expect("上升沿应触发");
        assert!(r.matched);
        assert!((r.value - 7.0).abs() < f32::EPSILON);
        // 持续高位: 不再触发
        assert!(st.eval_auto("rising", 5.0).is_none());
        assert!(st.eval_auto("rising", 6.0).is_none()); // 非零 → 非零 不算上升沿
                                                        // 回落到 0 后再升: 重新触发 (re-arm)
        assert!(st.eval_auto("rising", 0.0).is_none());
        assert!(st.eval_auto("rising", 3.0).is_some());
        // 负值不是上升沿 (prev==0 但 value 不 > 0)
        assert!(st.eval_auto("rising", 0.0).is_none());
        assert!(st.eval_auto("rising", -2.0).is_none());
    }

    #[test]
    fn auto_match_uses_value_string_as_command() {
        // 前端 runMatch(String(triggerValue)): 命令为数值字符串形式
        let mut st = TriggerState::new(
            vec![rule("r1", TriggerMatchType::Exact, "5", 1.0, true)],
            0.0,
            String::new(),
        );
        assert!(st.eval_auto("level", 5.0).is_some_and(|r| r.matched));
        let mut st = TriggerState::new(
            vec![rule("r1", TriggerMatchType::Exact, "5", 1.0, true)],
            0.0,
            String::new(),
        );
        assert!(st.eval_auto("level", 5.5).is_some_and(|r| !r.matched));
    }

    #[test]
    fn manual_eval_parses_numeric_for_range() {
        let mut st = state_with_range_rule();
        assert!(st.eval_manual("5").matched); // 可解析为数值 → Range 命中
        assert!(!st.eval_manual("abc").matched); // 非数值 → Range 跳过
        assert!(!st.eval_manual("").matched); // 空串 → None
    }

    #[test]
    fn config_change_detection() {
        let rules = vec![rule("r1", TriggerMatchType::Exact, "X", 1.0, true)];
        let st = TriggerState::new(rules.clone(), 0.0, "miss".to_string());
        assert!(st.matches_config(&rules, 0.0, "miss"));
        assert!(!st.matches_config(&rules, 1.0, "miss")); // default_miss 变化
        assert!(!st.matches_config(&rules, 0.0, "other")); // default_miss_text 变化
        let rules2 = vec![rule("r1", TriggerMatchType::Exact, "Y", 1.0, true)];
        assert!(!st.matches_config(&rules2, 0.0, "miss")); // rules 变化
    }

    #[test]
    fn format_auto_command_js_compat() {
        assert_eq!(format_auto_command(0.0), "0");
        assert_eq!(format_auto_command(-0.0), "0"); // JS String(-0) === "0"
        assert_eq!(format_auto_command(5.0), "5"); // 整数不带 .0
        assert_eq!(format_auto_command(1.5), "1.5");
        assert_eq!(format_auto_command(f32::NAN), "NaN");
        assert_eq!(format_auto_command(f32::INFINITY), "Infinity");
        assert_eq!(format_auto_command(f32::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn record_prev_prevents_false_rising_after_manual() {
        // manual 期间输入 0→5 (record_prev 跟踪); 切回 auto+rising 且输入保持 5 → 不触发
        let mut st = state_with_range_rule();
        st.record_prev(0.0);
        st.record_prev(5.0);
        assert!(
            st.eval_auto("rising", 5.0).is_none(),
            "prev=5 不应误判上升沿"
        );
        // 回落后再升: 正常触发
        assert!(st.eval_auto("rising", 0.0).is_none());
        assert!(st.eval_auto("rising", 5.0).is_some());
    }
}
