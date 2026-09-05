//! 模式编译与解析 — 正则 flags 映射 / Range 解析 / glob→正则展开

use regex::Regex;

/// 把 JS 风格 flags 串 (`"i"` / `"im"` / `"ims"`) 映射到 `regex::RegexBuilder`
/// 支持的 flag (case_insensitive / multi_line / dot_matches_new_line / swap_greed)
pub(super) fn compile_regex(pattern: &str, flags: &str) -> Result<Regex, regex::Error> {
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
pub(super) fn glob_to_regex_source(glob: &str) -> String {
    format!("^{}$", glob_to_regex_source_inner(glob))
}

#[cfg(test)]
mod tests {
    use super::parse_range;

    #[test]
    fn parse_range_basic() {
        assert_eq!(parse_range("1..10"), Some((1.0, 10.0)));
        assert_eq!(parse_range("-5.5..3.75"), Some((-5.5, 3.75)));
        assert_eq!(parse_range("-10..0"), Some((-10.0, 0.0)));
    }

    #[test]
    fn parse_range_infinity() {
        assert_eq!(
            parse_range("-Infinity..Infinity"),
            Some((f32::NEG_INFINITY, f32::INFINITY))
        );
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
}
