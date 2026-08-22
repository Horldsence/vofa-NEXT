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

use std::collections::HashMap;

use glob::Pattern;
use regex::Regex;
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

/// 触发器匹配器 (无状态, 每次命令构建一次或复用)
pub struct TriggerMatcher {
    rules: Vec<TriggerRuleDef>,
    default_miss: f32,
    default_miss_text: String,
    /// 按 rule.id 缓存编译后的正则 (`None` = 上次编译失败, 视为不命中)
    regex_cache: HashMap<String, Option<Regex>>,
    /// 按 rule.id 缓存编译后的 glob (同上)
    glob_cache: HashMap<String, Option<Pattern>>,
}

impl TriggerMatcher {
    #[must_use]
    pub fn new(rules: Vec<TriggerRuleDef>, default_miss: f32, default_miss_text: String) -> Self {
        Self {
            rules,
            default_miss,
            default_miss_text,
            regex_cache: HashMap::new(),
            glob_cache: HashMap::new(),
        }
    }

    /// 执行一次匹配
    ///
    /// - `command`: 待匹配命令字符串
    /// - `numeric`: 用于 `Range` 匹配的数值; `None` 时跳过 Range 规则
    ///
    /// 返回首个命中规则 (按 output_type 取对应字段) 或默认 miss。
    pub fn match_input(&mut self, command: &str, numeric: Option<f32>) -> TriggerMatchResult {
        // 借用规则副本避免与 `&mut self` 冲突 (regex/glob cache 写入需可变借用)
        let snapshot = self.rules.clone();
        for rule in &snapshot {
            if !rule.enabled {
                continue;
            }
            let hit = self.eval_rule(rule, command, numeric);
            if hit {
                if rule.output_type == "string" {
                    return TriggerMatchResult {
                        value: self.default_miss,
                        matched: true,
                        text: rule.output_text.clone(),
                        output_type: "string".to_string(),
                    };
                }
                return TriggerMatchResult {
                    value: rule.output_value,
                    matched: true,
                    text: self.default_miss_text.clone(),
                    output_type: "number".to_string(),
                };
            }
        }
        TriggerMatchResult {
            value: self.default_miss,
            matched: false,
            text: self.default_miss_text.clone(),
            output_type: "miss".to_string(),
        }
    }

    fn eval_rule(
        &mut self,
        rule: &TriggerRuleDef,
        command: &str,
        numeric: Option<f32>,
    ) -> bool {
        match rule.match_type {
            TriggerMatchType::Exact => command == rule.pattern,
            TriggerMatchType::Prefix => command.starts_with(&rule.pattern),
            TriggerMatchType::Contains => command.contains(&rule.pattern),
            TriggerMatchType::Regex => self
                .get_or_compile_regex(&rule.id, &rule.pattern, rule.flags.as_deref().unwrap_or(""))
                .is_some_and(|re| re.is_match(command)),
            TriggerMatchType::Range => numeric.is_some_and(|n| {
                parse_range(&rule.pattern).is_some_and(|(lo, hi)| n >= lo && n <= hi)
            }),
            TriggerMatchType::Glob => {
                if rule.pattern.contains('{') {
                    // brace-expansion: 把 `{a,b}` 编译为正则 (缓存 key: "{id}:g")
                    let key = format!("{}:g", rule.id);
                    if !self.regex_cache.contains_key(&key) {
                        let regex_src = glob_to_regex_source(&rule.pattern);
                        let compiled = compile_regex(&regex_src, "").ok();
                        self.regex_cache.insert(key.clone(), compiled);
                    }
                    self.regex_cache
                        .get(&key)
                        .and_then(Option::as_ref)
                        .is_some_and(|re| re.is_match(command))
                } else {
                    self.get_or_compile_glob(&rule.id, &rule.pattern)
                        .is_some_and(|p| p.matches(command))
                }
            }
        }
    }

    /// 取缓存或编译; 失败时缓存 `None` 并返回之 (视为不命中, 不抛错)
    fn get_or_compile_regex(&mut self, id: &str, pattern: &str, flags: &str) -> Option<&Regex> {
        if !self.regex_cache.contains_key(id) {
            let compiled = match compile_regex(pattern, flags) {
                Ok(re) => Some(re),
                Err(e) => {
                    log::warn!(
                        "trigger rule {id}: invalid regex pattern {pattern:?} flags {flags:?}: {e}"
                    );
                    None
                }
            };
            self.regex_cache.insert(id.to_string(), compiled);
        }
        self.regex_cache.get(id).and_then(Option::as_ref)
    }

    /// 同上, glob 版 (仅处理不含 `{...}` 的模式; 含 brace 的由 eval_rule 单独分支处理)
    fn get_or_compile_glob(&mut self, id: &str, pattern: &str) -> Option<&Pattern> {
        if !self.glob_cache.contains_key(id) {
            let compiled = match Pattern::new(pattern) {
                Ok(p) => Some(p),
                Err(e) => {
                    log::warn!("trigger rule {id}: invalid glob pattern {pattern:?}: {e}");
                    None
                }
            };
            self.glob_cache.insert(id.to_string(), compiled);
        }
        self.glob_cache.get(id).and_then(Option::as_ref)
    }
}

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

/// 把 JS 风格 flags 串 (`"i"` / `"im"` / `"ims"`) 映射到 `regex::RegexBuilder`
/// 支持的 flag (case_insensitive / multi_line / dot_matches_new_line / swap_greed)
fn compile_regex(pattern: &str, flags: &str) -> Result<Regex, regex::Error> {
    let mut builder = regex::RegexBuilder::new(pattern);
    let mut case_insensitive = false;
    let mut multi_line = false;
    let mut dot_matches_new_line = false;
    let mut swap_greed = false;
    for c in flags.chars() {
        match c {
            'i' | 'I' => case_insensitive = true,
            'm' | 'M' => multi_line = true,
            's' | 'S' => dot_matches_new_line = true,
            'U' => swap_greed = true,
            _ => {} // 忽略未知 flag (与 JS RegExp 行为兼容)
        }
    }
    if case_insensitive {
        builder.case_insensitive(true);
    }
    if multi_line {
        builder.multi_line(true);
    }
    if dot_matches_new_line {
        builder.dot_matches_new_line(true);
    }
    if swap_greed {
        builder.swap_greed(true);
    }
    builder.build()
}

/// 范围模式解析: 支持 `min..max`, 端点为整数 / 小数 / `Infinity` / `-Infinity`
///
/// 返回 `(min, max)` (f32, 已含端点)。格式错误或 `min > max` 时返回 `None`。
#[must_use]
pub fn parse_range(pattern: &str) -> Option<(f32, f32)> {
    let (lo_str, hi_str) = pattern.split_once("..")?;
    let lo = parse_bound(lo_str)?;
    let hi = parse_bound(hi_str)?;
    if lo > hi {
        return None;
    }
    Some((lo, hi))
}

fn parse_bound(s: &str) -> Option<f32> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("infinity") {
        Some(f32::INFINITY)
    } else if s.eq_ignore_ascii_case("-infinity") {
        Some(f32::NEG_INFINITY)
    } else {
        s.parse::<f32>().ok()
    }
}

/// 把 glob 模式展开为正则源码 — 支持 `*` `?` `[abc]` `{a,b}` (含转义)
/// 输出两端不带 `^...$` 锚点 — 由调用方包裹
fn glob_to_regex_source_inner(glob: &str) -> String {
    let mut out = String::with_capacity(glob.len() * 2);
    let mut chars = glob.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            '[' => {
                out.push('[');
                while let Some(&nc) = chars.peek() {
                    out.push(nc);
                    chars.next();
                    if nc == ']' {
                        break;
                    }
                }
            }
            '{' => {
                let mut group = String::new();
                let mut depth = 1;
                for nc in chars.by_ref() {
                    if nc == '{' {
                        depth += 1;
                    } else if nc == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    group.push(nc);
                }
                let alts: Vec<String> = group.split(',').map(glob_to_regex_source_inner).collect();
                out.push('(');
                out.push_str(&alts.join("|"));
                out.push(')');
            }
            '.' | '(' | ')' | '+' | '|' | '^' | '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// 把 glob 模式转为带 `^...$` 锚点的正则源码
fn glob_to_regex_source(glob: &str) -> String {
    format!("^{}$", glob_to_regex_source_inner(glob))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, mt: TriggerMatchType, pattern: &str, value: f32, enabled: bool) -> TriggerRuleDef {
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

    fn string_rule(id: &str, mt: TriggerMatchType, pattern: &str, text: &str, enabled: bool) -> TriggerRuleDef {
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

    #[test]
    fn parse_range_basic() {
        assert_eq!(parse_range("1..10"), Some((1.0, 10.0)));
        assert_eq!(parse_range("-5.5..3.14"), Some((-5.5, 3.14)));
        assert_eq!(parse_range("-10..0"), Some((-10.0, 0.0)));
    }

    #[test]
    fn parse_range_infinity() {
        assert_eq!(parse_range("-Infinity..Infinity"), Some((f32::NEG_INFINITY, f32::INFINITY)));
        assert_eq!(parse_range("0..Infinity"), Some((0.0, f32::INFINITY)));
        assert_eq!(parse_range("-Infinity..0"), Some((f32::NEG_INFINITY, 0.0)));
    }

    #[test]
    fn parse_range_invalid() {
        assert_eq!(parse_range("10..1"), None); // 反向
        assert_eq!(parse_range("abc..xyz"), None); // 非数字
        assert_eq!(parse_range("1.."), None); // 缺上限
        assert_eq!(parse_range("..10"), None); // 缺下限
        assert_eq!(parse_range(""), None);
        assert_eq!(parse_range("1.2.3..5"), None); // 多点号
    }

    #[test]
    fn match_exact() {
        let rules = vec![rule("r1", TriggerMatchType::Exact, "HELLO", 1.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert!(m.match_input("HELLO", None).matched);
        assert_eq!(m.match_input("HELLO", None).value, 1.0);
        assert!(!m.match_input("hello", None).matched); // 大小写敏感
        assert!(!m.match_input("", None).matched);
    }

    #[test]
    fn match_prefix_contains() {
        let rules = vec![
            rule("r1", TriggerMatchType::Prefix, "GET", 1.0, true),
            rule("r2", TriggerMatchType::Contains, "TEMP", 2.0, true),
        ];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert_eq!(m.match_input("GET_TEMP", None).value, 1.0); // 首个命中
        assert_eq!(m.match_input("SET_TEMP", None).value, 2.0);
        assert_eq!(m.match_input("SET_VOLT", None).value, 0.0); // 默认未命中
    }

    #[test]
    fn match_regex() {
        let rules = vec![rule("r1", TriggerMatchType::Regex, r"^H.*O$", 5.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert_eq!(m.match_input("HELLO", None).value, 5.0);
        assert_eq!(m.match_input("HEYLO", None).value, 5.0); // .* 匹配任意
        assert!(!m.match_input("HELP", None).matched); // 不以 O 结尾
        assert!(!m.match_input("AYO", None).matched); // 不以 H 开头
    }

    #[test]
    fn match_regex_flags_case_insensitive() {
        let mut r = rule("r1", TriggerMatchType::Regex, "hello", 1.0, true);
        r.flags = Some("i".to_string());
        let mut m = TriggerMatcher::new(vec![r], 0.0, String::new());
        assert!(m.match_input("HELLO", None).matched);
        assert!(m.match_input("Hello", None).matched);
    }

    #[test]
    fn match_regex_invalid_silent() {
        // 无效正则 (未闭合的字符类) — 不应 panic, 视为不命中
        let rules = vec![rule("r1", TriggerMatchType::Regex, "[", 9.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        let r = m.match_input("anything", None);
        assert!(!r.matched);
        assert_eq!(r.value, 0.0);
    }

    #[test]
    fn match_range_boundaries() {
        let rules = vec![rule("r1", TriggerMatchType::Range, "1..5", 7.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        // 包含端点
        assert!(m.match_input("", Some(1.0)).matched);
        assert!(m.match_input("", Some(5.0)).matched);
        assert!(m.match_input("", Some(3.14)).matched);
        assert!(!m.match_input("", Some(0.999)).matched);
        assert!(!m.match_input("", Some(5.001)).matched);
        // 无 numeric 时跳过
        assert!(!m.match_input("", None).matched);
    }

    #[test]
    fn match_disabled_rule_skipped() {
        let rules = vec![
            rule("r1", TriggerMatchType::Exact, "X", 1.0, false), // 禁用
            rule("r2", TriggerMatchType::Exact, "Y", 2.0, true),
        ];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert_eq!(m.match_input("X", None).value, 0.0); // r1 禁用, 默认值
        assert_eq!(m.match_input("Y", None).value, 2.0);
    }

    #[test]
    fn match_first_hit_wins() {
        let rules = vec![
            rule("r1", TriggerMatchType::Contains, "T", 1.0, true),
            rule("r2", TriggerMatchType::Exact, "TEST", 2.0, true),
        ];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert_eq!(m.match_input("TEST", None).value, 1.0); // r1 先命中
    }

    #[test]
    fn match_no_rules_returns_default_miss() {
        let mut m = TriggerMatcher::new(vec![], 99.0, String::new());
        assert!(!m.match_input("anything", None).matched);
        assert_eq!(m.match_input("anything", None).value, 99.0);
    }

    #[test]
    fn match_string_output_rule() {
        let rules = vec![string_rule("r1", TriggerMatchType::Exact, "HELLO", "world", true)];
        let mut m = TriggerMatcher::new(rules, 0.0, "fallback".to_string());
        let r = m.match_input("HELLO", None);
        assert!(r.matched);
        assert_eq!(r.output_type, "string");
        assert_eq!(r.text, "world");
        // number 字段填 miss (命中规则未指定 number 输出)
        assert_eq!(r.value, 0.0);
    }

    #[test]
    fn match_string_default_miss_returns_default_miss_text() {
        let mut m = TriggerMatcher::new(vec![], 42.0, "empty".to_string());
        let r = m.match_input("anything", None);
        assert!(!r.matched);
        assert_eq!(r.value, 42.0);
        assert_eq!(r.text, "empty");
        assert_eq!(r.output_type, "miss");
    }

    #[test]
    fn match_glob_star() {
        let rules = vec![rule("r1", TriggerMatchType::Glob, "GET_*", 1.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert!(m.match_input("GET_TEMP", None).matched);
        assert!(m.match_input("GET_VOLT", None).matched);
        assert!(!m.match_input("SET_TEMP", None).matched);
    }

    #[test]
    fn match_glob_qmark() {
        let rules = vec![rule("r1", TriggerMatchType::Glob, "?_TEMP", 1.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert!(m.match_input("A_TEMP", None).matched);
        assert!(m.match_input("X_TEMP", None).matched);
        assert!(!m.match_input("AB_TEMP", None).matched); // 通配单字符
    }

    #[test]
    fn match_glob_charset() {
        let rules = vec![rule("r1", TriggerMatchType::Glob, "[GS]ET_*", 1.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert!(m.match_input("GET_TEMP", None).matched);
        assert!(m.match_input("SET_VOLT", None).matched);
        assert!(!m.match_input("XET_VOLT", None).matched);
    }

    #[test]
    fn match_glob_alternatives() {
        let rules = vec![rule("r1", TriggerMatchType::Glob, "{HELLO,HI}_*", 1.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert!(m.match_input("HELLO_TEMP", None).matched);
        assert!(m.match_input("HI_VOLT", None).matched);
        assert!(!m.match_input("BYE_TEMP", None).matched);
    }

    #[test]
    fn match_glob_invalid_silent() {
        // 无效 glob (未闭合字符类) — 不应 panic, 视为不命中
        let rules = vec![rule("r1", TriggerMatchType::Glob, "[", 9.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        let r = m.match_input("anything", None);
        assert!(!r.matched);
        assert_eq!(r.value, 0.0);
    }

    #[test]
    fn glob_cache_reuses_compile() {
        let rules = vec![rule("r1", TriggerMatchType::Glob, "GET_*", 1.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert!(m.match_input("GET_TEMP", None).matched);
        assert!(m.match_input("GET_VOLT", None).matched);
        assert_eq!(m.glob_cache.len(), 1);
    }

    #[test]
    fn regex_cache_reuses_compile() {
        // 同一 rule.id 多次调用 — 仅编译一次
        let rules = vec![rule("r1", TriggerMatchType::Regex, r"\d+", 1.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert!(m.match_input("abc1", None).matched);
        assert!(m.match_input("abc2", None).matched);
        assert_eq!(m.regex_cache.len(), 1);
    }

    // ============ TriggerState: 边沿检测 + 求值状态 ============

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
}
