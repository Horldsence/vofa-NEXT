//! `StrOp` 评估 — 端口表已知, 字符串 + 数值输入已收集, 返回 `StrResult`.

use super::StrResult;

/// 数值端口值 → 字符计数: `round()` 后 clamp 到 `>= 0`
/// (f32 → usize 为饱和转换, NaN/负数归 0, 超大值归 `usize::MAX`)
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn to_count(v: f32) -> usize {
    v.round().max(0.0) as usize
}

/// 字符数（按 chars 计，非字节数）
fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// 取字符区间 `[start, end)`（0-based 字符索引，越界自动截断）
fn char_slice(s: &str, start: usize, end: usize) -> String {
    s.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

/// 字符串操作评估实现 — 内部分发到 `match`, 调用方按 `input_ports_for()` 顺序塞入输入.
#[allow(clippy::cast_precision_loss)]
pub fn evaluate(op: super::StrOp, str_inputs: &[&str], num_inputs: &[f32]) -> StrResult {
    use super::StrOp::*;
    let s = |i: usize| str_inputs.get(i).copied().unwrap_or("");
    let n = |i: usize| num_inputs.get(i).copied().unwrap_or(0.0);
    match op {
        Len => StrResult::Num(char_len(s(0)) as f32),
        Find => StrResult::Num(s(0).find(s(1)).map_or(0.0, |byte_idx| {
            (s(0)[..byte_idx].chars().count() + 1) as f32
        })),
        Contains => StrResult::Num(if s(0).contains(s(1)) { 1.0 } else { 0.0 }),
        Left => {
            let size = to_count(n(0));
            StrResult::Text(if size == 0 {
                s(0).to_owned()
            } else {
                char_slice(s(0), 0, size)
            })
        }
        Right => {
            let size = to_count(n(0));
            let len = char_len(s(0));
            StrResult::Text(if size == 0 {
                s(0).to_owned()
            } else {
                char_slice(s(0), len.saturating_sub(size), len)
            })
        }
        Mid => {
            let src = s(0);
            let len = char_len(src);
            let start = to_count(n(0)).clamp(1, len + 1) - 1;
            let count = to_count(n(1));
            let end = if count == 0 {
                len
            } else {
                start.saturating_add(count)
            };
            StrResult::Text(char_slice(src, start, end))
        }
        Concat => StrResult::Text({
            let (a, b) = (s(0), s(1));
            format!("{a}{b}")
        }),
        Insert => {
            let src = s(0);
            let len = char_len(src);
            let start = to_count(n(0)).clamp(1, len + 1) - 1;
            let (head, mid, tail) = (char_slice(src, 0, start), s(1), char_slice(src, start, len));
            StrResult::Text(format!("{head}{mid}{tail}"))
        }
        Delete => {
            let src = s(0);
            let len = char_len(src);
            let pos = to_count(n(0));
            if pos > len {
                StrResult::Text(src.to_owned())
            } else {
                let start = pos.max(1) - 1;
                let count = to_count(n(1));
                let end = if count == 0 {
                    len
                } else {
                    start.saturating_add(count)
                };
                let (head, tail) = (char_slice(src, 0, start), char_slice(src, end, len));
                StrResult::Text(format!("{head}{tail}"))
            }
        }
        Replace => {
            let src = s(0);
            let len = char_len(src);
            let pos = to_count(n(0));
            if pos > len {
                StrResult::Text(src.to_owned())
            } else {
                let start = pos.max(1) - 1;
                let count = to_count(n(1));
                let end = if count == 0 {
                    len
                } else {
                    start.saturating_add(count)
                };
                let (head, mid, tail) =
                    (char_slice(src, 0, start), s(1), char_slice(src, end, len));
                StrResult::Text(format!("{head}{mid}{tail}"))
            }
        }
        Upper => StrResult::Text(s(0).to_uppercase()),
        Lower => StrResult::Text(s(0).to_lowercase()),
        Trim => StrResult::Text(s(0).trim().to_owned()),
        Reverse => StrResult::Text(s(0).chars().rev().collect()),
    }
}
