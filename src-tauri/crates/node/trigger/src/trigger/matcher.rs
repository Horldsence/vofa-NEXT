//! 触发器匹配器 — 规则顺序求值 + 正则/glob 编译缓存

use std::collections::HashMap;

use glob::Pattern;
use regex::Regex;

use super::pattern::{compile_regex, glob_to_regex_source, parse_range};
use super::types::{TriggerMatchResult, TriggerMatchType, TriggerRuleDef};

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

    fn eval_rule(&mut self, rule: &TriggerRuleDef, command: &str, numeric: Option<f32>) -> bool {
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

#[cfg(test)]
mod tests {
    use super::super::test_util::{assert_value, rule, string_rule};
    use super::*;

    #[test]
    fn match_exact() {
        let rules = vec![rule("r1", TriggerMatchType::Exact, "HELLO", 1.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert!(m.match_input("HELLO", None).matched);
        assert_value(m.match_input("HELLO", None).value, 1.0);
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
        assert_value(m.match_input("GET_TEMP", None).value, 1.0); // 首个命中
        assert_value(m.match_input("SET_TEMP", None).value, 2.0);
        assert_value(m.match_input("SET_VOLT", None).value, 0.0); // 默认未命中
    }

    #[test]
    fn match_regex() {
        let rules = vec![rule("r1", TriggerMatchType::Regex, r"^H.*O$", 5.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert_value(m.match_input("HELLO", None).value, 5.0);
        assert_value(m.match_input("HEYLO", None).value, 5.0); // .* 匹配任意
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
        assert_value(r.value, 0.0);
    }

    #[test]
    fn match_range_boundaries() {
        let rules = vec![rule("r1", TriggerMatchType::Range, "1..5", 7.0, true)];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        // 包含端点
        assert!(m.match_input("", Some(1.0)).matched);
        assert!(m.match_input("", Some(5.0)).matched);
        assert!(m.match_input("", Some(3.25)).matched);
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
        assert_value(m.match_input("X", None).value, 0.0); // r1 禁用, 默认值
        assert_value(m.match_input("Y", None).value, 2.0);
    }

    #[test]
    fn match_first_hit_wins() {
        let rules = vec![
            rule("r1", TriggerMatchType::Contains, "T", 1.0, true),
            rule("r2", TriggerMatchType::Exact, "TEST", 2.0, true),
        ];
        let mut m = TriggerMatcher::new(rules, 0.0, String::new());
        assert_value(m.match_input("TEST", None).value, 1.0); // r1 先命中
    }

    #[test]
    fn match_no_rules_returns_default_miss() {
        let mut m = TriggerMatcher::new(vec![], 99.0, String::new());
        assert!(!m.match_input("anything", None).matched);
        assert_value(m.match_input("anything", None).value, 99.0);
    }

    #[test]
    fn match_string_output_rule() {
        let rules = vec![string_rule(
            "r1",
            TriggerMatchType::Exact,
            "HELLO",
            "world",
            true,
        )];
        let mut m = TriggerMatcher::new(rules, 0.0, "fallback".to_string());
        let r = m.match_input("HELLO", None);
        assert!(r.matched);
        assert_eq!(r.output_type, "string");
        assert_eq!(r.text, "world");
        // number 字段填 miss (命中规则未指定 number 输出)
        assert_value(r.value, 0.0);
    }

    #[test]
    fn match_string_default_miss_returns_default_miss_text() {
        let mut m = TriggerMatcher::new(vec![], 42.0, "empty".to_string());
        let r = m.match_input("anything", None);
        assert!(!r.matched);
        assert_value(r.value, 42.0);
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
        let rules = vec![rule(
            "r1",
            TriggerMatchType::Glob,
            "{HELLO,HI}_*",
            1.0,
            true,
        )];
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
        assert_value(r.value, 0.0);
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
}
