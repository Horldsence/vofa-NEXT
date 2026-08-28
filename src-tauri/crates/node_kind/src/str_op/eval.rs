//! `StrOp` 评估 — 端口表已知, 字符串 + 数值输入已收集, 返回 `StrResult`.

use std::fmt::Write as _;

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
        // FORMAT: 模板 (fmt 端口, 未连接走 inline 回退) + in0..in3 → {N}/{N:.P} 展开
        // 缺失的数值输入按 0.0 展开 (与其余算子的 n() 取参语义一致)
        Format => {
            let nums = [n(0), n(1), n(2), n(3)];
            StrResult::Text(format_template(s(0), &nums))
        }
        // PARSE: 从 pos 起 (1-based 字符) 扫描首个数字 token; 未命中返回 0.0
        Parse => {
            let src = s(0);
            let len = char_len(src);
            if len == 0 {
                return StrResult::Num(0.0);
            }
            let start = to_count(n(0)).clamp(1, len + 1) - 1;
            let rest: String = src.chars().skip(start).collect();
            StrResult::Num(scan_first_number(&rest))
        }
        // ENCODE_HEX: UTF-8 字节大写 HEX (逐字节直写, 避免 map+collect 的中间分配)
        EncodeHex => {
            let mut hex = String::with_capacity(s(0).len() * 2);
            for b in s(0).bytes() {
                let _ = write!(hex, "{b:02X}");
            }
            StrResult::Text(hex)
        }
    }
}

/// FORMAT 模板展开 — `{N}` 引用第 N 路 (0-based), `{N:.P}` 定精度, `{{`/`}}` 字面转义.
///
/// 数值默认按 f32 最短表示 (`Display`, 如 `1.5` → "1.5"); 无法解析的片段
/// 原样输出 (防御性: 不吞字、不报错)。索引越界同样原样输出。
fn format_template(tmpl: &str, nums: &[f32]) -> String {
    let mut out = String::with_capacity(tmpl.len() + 16);
    let mut chars = tmpl.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    out.push('{');
                    continue;
                }
                // 收集到 '}' 为止的 token 内容 ('}' 留在迭代器中)
                let mut tok = String::new();
                let mut closed = false;
                while let Some(&t) = chars.peek() {
                    if t == '}' {
                        chars.next();
                        closed = true;
                        break;
                    }
                    tok.push(t);
                    chars.next();
                }
                if !closed {
                    // 未闭合 '{': 按字面输出 ('}' 未消费, 自然跟随)
                    out.push('{');
                    out.push_str(&tok);
                    continue;
                }
                match expand_token(&tok, nums) {
                    Some(text) => out.push_str(&text),
                    // 非法/越界引用: "{tok}" 原样输出
                    None => {
                        out.push('{');
                        out.push_str(&tok);
                        out.push('}');
                    }
                }
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                }
                out.push('}');
            }
            _ => out.push(c),
        }
    }
    out
}

/// 单个 `{...}` token 展开 — "N" 或 "N:.P"; 成功返回替换文本
fn expand_token(tok: &str, nums: &[f32]) -> Option<String> {
    let (idx_part, precision) = match tok.split_once(':') {
        // ":.P" 形式 (':' 后必须紧跟 '.')
        Some((idx, spec)) => {
            let p = spec.strip_prefix('.')?;
            let digits_ok = !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
            if !digits_ok {
                return None;
            }
            #[allow(clippy::cast_possible_truncation)] // 精度位数受限于 token 长度
            (idx, p.parse::<usize>().ok())
        }
        None => (tok, None),
    };
    let idx: usize = idx_part.parse().ok()?;
    let v = *nums.get(idx)?;
    Some(precision.map_or_else(|| format!("{v}"), |p| format!("{v:.p$}")))
}

/// PARSE 数字扫描 — 返回文本中首个数字 token 的 f32 值; 未命中返回 0.0.
///
/// 支持形式: `[+-]?十进制` (`123` / `-1.5` / `.5` / `2.` / `+3e-2` 指数可选)、
/// 无符号十六进制 `0x1A2B`。token 起点限定 ASCII `- + . 0-9`, 多字节字符安全。
#[allow(clippy::cast_possible_truncation)] // f64 累计值输出为 f32 槽位, 截断可接受
fn scan_first_number(text: &str) -> f32 {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if is_number_start(b[i]) {
            if let Some((_, v)) = number_at(b, i) {
                return v as f32;
            }
        }
        i += 1;
    }
    0.0
}

#[allow(clippy::cast_sign_loss)]
const fn is_number_start(byte: u8) -> bool {
    byte.is_ascii_digit()
        || matches!(byte, b'+' | b'-')
        || (byte == b'.')
}

/// 从 `text[from..]` 的字节偏移处解析最长数字 token → (长度, f64)
///
/// 十六进制优先识别 `0x`(无符号); 其余按十进制含指数解析。无有效 token 返回 None。
fn number_at(text: &[u8], from: usize) -> Option<(usize, f64)> {
    let b = text.get(from..)?;
    // 符号位 (仅十进制取符号; 0x 十六进制不取负号)
    let mut k = 0;
    let sign_neg = b.first() == Some(&b'-');
    if matches!(b.first(), Some(b'+' | b'-')) {
        k = 1;
        // "+"/"-" 后必须是数字或 ".数字", 否则该符号不是 token 起点
        let d = b.get(k).copied().unwrap_or(b' ');
        let dd = b.get(k + 1).copied().unwrap_or(b' ');
        if !(d.is_ascii_digit() || (d == b'.' && dd.is_ascii_digit())) {
            return None;
        }
    }
    // hex: 0x 前缀 (仅无符号)
    if k == 0 && b.len() > 2 && b[0] == b'0' && (b[1] | 0x20) == b'x' && b[2].is_ascii_hexdigit() {
        let mut m = 2;
        while m < b.len() && b[m].is_ascii_hexdigit() {
            m += 1;
        }
        // 逐位累计避免超长串溢出 (f64 有效位足够表达常规协议地址)
        let mut acc: f64 = 0.0;
        for &h in &b[2..m] {
            let d = (char::from(h)).to_digit(16).unwrap_or(0);
            acc = acc.mul_add(16.0, f64::from(d));
        }
        return Some((m, acc));
    }
    // 十进制: 整数部分 / 小数部分 / 指数
    let start = k;
    let mut m = k;
    let mut seen_digit = false;
    while m < b.len() && b[m].is_ascii_digit() {
        m += 1;
        seen_digit = true;
    }
    if m < b.len() && b[m] == b'.' {
        m += 1;
        while m < b.len() && b[m].is_ascii_digit() {
            m += 1;
            seen_digit = true;
        }
    }
    if !seen_digit {
        return None;
    }
    // 指数: 仅当 e/E 后跟 [±]数字 才纳入 (否则 e 属于普通文本)
    if m < b.len() && (b[m] | 0x20) == b'e' {
        let mut n = m + 1;
        if n < b.len() && matches!(b[n], b'+' | b'-') {
            n += 1;
        }
        if n < b.len() && b[n].is_ascii_digit() {
            while n < b.len() && b[n].is_ascii_digit() {
                n += 1;
            }
            m = n;
        }
    }
    let token = std::str::from_utf8(&b[start..m]).ok()?;
    let value: f64 = token.parse().ok()?;
    Some((m, if sign_neg { -value } else { value }))
}

#[cfg(test)]
mod tests {
    // PARSE 结果均为小整数/精确表示的 f32, assert_eq! 严格比较是有意为之
    #![allow(clippy::float_cmp)]

    use crate::ports::PortDomain;
    use crate::str_op::{StrOp, StrResult};

    fn text(r: StrResult) -> String {
        match r {
            StrResult::Text(t) => t,
            StrResult::Num(n) => panic!("expected Text, got Num({n})"),
        }
    }

    fn num(r: StrResult) -> f32 {
        match r {
            StrResult::Num(n) => n,
            StrResult::Text(t) => panic!("expected Num, got Text({t:?})"),
        }
    }

    fn evaluate(op: StrOp, str_inputs: &[&str], num_inputs: &[f32]) -> StrResult {
        op.evaluate(str_inputs, num_inputs)
    }

    #[test]
    fn format_basic_refs_and_escapes() {
        assert_eq!(
            text(evaluate(StrOp::Format, &["{0}={1}"], &[1.5, -2.25])),
            "1.5=-2.25"
        );
        assert_eq!(
            text(evaluate(StrOp::Format, &["{{}}{0}"], &[7.0])),
            "{}7"
        );
        // 越界/非法引用原样输出
        assert_eq!(text(evaluate(StrOp::Format, &["{5}"], &[1.0])), "{5}");
        assert_eq!(text(evaluate(StrOp::Format, &["{a}"], &[1.0])), "{a}");
        assert_eq!(text(evaluate(StrOp::Format, &["{0"], &[1.0])), "{0");
        assert_eq!(text(evaluate(StrOp::Format, &["}"], &[])), "}");
    }

    #[test]
    #[allow(clippy::literal_string_with_formatting_args)] // 模板字面量本就是被测对象
    fn format_precision_and_missing_inputs() {
        assert_eq!(
            text(evaluate(StrOp::Format, &["{0:.2}|{1:.0}"], &[3.5, 9.6])),
            "3.50|10"
        );
        // 未连接的数值输入缺省 0.0 (由调用方收参保证; 这里直接验证缺省行为)
        assert_eq!(text(evaluate(StrOp::Format, &["v={0}"], &[])), "v=0");
        // 未连接 fmt 缺省空模板 → 输出空串
        assert_eq!(text(evaluate(StrOp::Format, &[], &[1.0])), "");
    }

    #[test]
    fn parse_decimal_forms() {
        assert_eq!(num(evaluate(StrOp::Parse, &["temp=12.75,C"], &[])), 12.75);
        assert_eq!(num(evaluate(StrOp::Parse, &["x-3e2y"], &[])), -300.0);
        assert_eq!(num(evaluate(StrOp::Parse, &[".5 and 2"], &[])), 0.5);
        assert_eq!(num(evaluate(StrOp::Parse, &["7."], &[])), 7.0);
        assert_eq!(num(evaluate(StrOp::Parse, &["abc"], &[])), 0.0);
        assert_eq!(num(evaluate(StrOp::Parse, &[""], &[])), 0.0);
    }

    #[test]
    fn parse_hex_forms() {
        assert_eq!(num(evaluate(StrOp::Parse, &["id=0x1A2B end"], &[])), 6699.0);
        assert_eq!(num(evaluate(StrOp::Parse, &["0XFF"], &[])), 255.0);
        // 未命中 hex (无前缀字母结尾) 不误读
        assert_eq!(num(evaluate(StrOp::Parse, &["0x"], &[])), 0.0);
    }

    #[test]
    fn parse_pos_is_one_based_char_index() {
        // 从第 5 个字符起扫描, 跳过前面的 "v=1." 部分
        assert_eq!(num(evaluate(StrOp::Parse, &["v=1.234"], &[5.0])), 234.0);
        // 多字节安全: pos 按 chars 计
        assert_eq!(num(evaluate(StrOp::Parse, &["温度:36.8度"], &[4.0])), 36.8);
        // pos 越界 → 0
        assert_eq!(num(evaluate(StrOp::Parse, &["1.5"], &[99.0])), 0.0);
    }

    #[test]
    fn encode_hex_uppercase_utf8_bytes() {
        assert_eq!(text(evaluate(StrOp::EncodeHex, &["AB"], &[])), "4142");
        assert_eq!(
            text(evaluate(StrOp::EncodeHex, &["你"], &[])),
            "E4BDA0"
        );
        assert_eq!(text(evaluate(StrOp::EncodeHex, &[""], &[])), "");
    }

    #[test]
    fn new_ops_port_tables() {
        assert_eq!(
            StrOp::Format.input_ports(),
            &[
                ("fmt", PortDomain::String),
                ("in0", PortDomain::F32),
                ("in1", PortDomain::F32),
                ("in2", PortDomain::F32),
                ("in3", PortDomain::F32)
            ]
        );
        assert_eq!(
            StrOp::Parse.input_ports(),
            &[("str", PortDomain::String), ("pos", PortDomain::F32)]
        );
        assert_eq!(StrOp::EncodeHex.input_ports(), &[("str", PortDomain::String)]);
        assert_eq!(StrOp::Format.output_domain(), PortDomain::String);
        assert_eq!(StrOp::Parse.output_domain(), PortDomain::F32);
        assert_eq!(StrOp::EncodeHex.output_domain(), PortDomain::String);
    }

    #[test]
    fn new_ops_serde_names() {
        assert_eq!(serde_json::to_string(&StrOp::Format).unwrap(), "\"format\"");
        assert_eq!(serde_json::to_string(&StrOp::Parse).unwrap(), "\"parse\"");
        assert_eq!(
            serde_json::to_string(&StrOp::EncodeHex).unwrap(),
            "\"encode_hex\""
        );
        let op: StrOp = serde_json::from_str("\"encode_hex\"").unwrap();
        assert_eq!(op, StrOp::EncodeHex);
    }
}
