//! 字符串操作 — StrOp 枚举 + StrNumParams 内联参数 + evaluate 方法
//!
//! 与前端 types/index.ts 中的 StrOp 保持一致 (lowercase, 同 MathOp)
//!
//! 语义规范:
//! - 索引 1-based (POS 从 1 开始; FIND 命中返回 1-based 位置, 未找到返回 0)
//! - 数值参数 round() 后 clamp 到 >= 0; POS clamp 到 \[1, len+1\]
//! - LEN/SIZE = 0 表示 "到末尾/全部" (Left/Right SIZE=0 → 整串;
//!   Mid/Delete/Replace LEN=0 → 从 POS 到末尾)
//! - 越界/空输入不报错: 截取越界返回可用部分或空串;
//!   DELETE/REPLACE 的 POS 超出长度时为 no-op (返回原串)
//! - 字符串索引按 chars() 字符计, 不用字节索引 (多字节字符安全)

use serde::{Deserialize, Serialize};

use crate::node_kind::PortDomain;

/// 字符串操作种类
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StrOp {
    Len,
    Find,
    Contains,
    Left,
    Right,
    Mid,
    Concat,
    Insert,
    Delete,
    Replace,
    Upper,
    Lower,
    Trim,
    Reverse,
}

/// 字符串操作结果 — 文本或数值
#[derive(Debug, Clone, PartialEq)]
pub enum StrResult {
    Text(String),
    Num(f32),
}

/// 数值端口 (pos/len/size) 的内联默认值
///
/// 端口未连接时求值使用此处的回退值, 由前端内联框编辑后同步;
/// 端口已连接时忽略, 求值使用上游值。各 op 只用与自己相关的字段。
/// 默认值: pos = 1 (1-based 起点), len/size = 0 (到末尾/全部)。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StrNumParams {
    pub pos: f32,
    pub len: f32,
    pub size: f32,
}

impl Default for StrNumParams {
    fn default() -> Self {
        Self {
            pos: 1.0,
            len: 0.0,
            size: 0.0,
        }
    }
}

/// 数值端口值 → 字符计数: round() 后 clamp 到 >= 0
/// (f32→usize 为饱和转换, NaN/负数归 0, 超大值归 usize::MAX)
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn to_count(v: f32) -> usize {
    v.round().max(0.0) as usize
}

/// 字符数 (按 chars 计, 非字节数)
fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// 取字符区间 \[start, end) (0-based 字符索引, 越界自动截断)
fn char_slice(s: &str, start: usize, end: usize) -> String {
    s.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

// 端口表 (供 port_domain 与后续编译期使用的单一事实源)
const IN_STR: &[(&str, PortDomain)] = &[("str", PortDomain::String)];
const IN_STR_SUBSTR: &[(&str, PortDomain)] =
    &[("str", PortDomain::String), ("substr", PortDomain::String)];
const IN_STR_SIZE: &[(&str, PortDomain)] =
    &[("str", PortDomain::String), ("size", PortDomain::F32)];
const IN_STR_POS_LEN: &[(&str, PortDomain)] = &[
    ("str", PortDomain::String),
    ("pos", PortDomain::F32),
    ("len", PortDomain::F32),
];
const IN_STR1_STR2: &[(&str, PortDomain)] =
    &[("str1", PortDomain::String), ("str2", PortDomain::String)];
const IN_STR1_STR2_POS: &[(&str, PortDomain)] = &[
    ("str1", PortDomain::String),
    ("str2", PortDomain::String),
    ("pos", PortDomain::F32),
];
const IN_STR1_STR2_POS_LEN: &[(&str, PortDomain)] = &[
    ("str1", PortDomain::String),
    ("str2", PortDomain::String),
    ("pos", PortDomain::F32),
    ("len", PortDomain::F32),
];

impl StrOp {
    /// 输入端口表 (按固定顺序, evaluate 的 str_inputs/num_inputs 依此取参)
    pub const fn input_ports(&self) -> &'static [(&'static str, PortDomain)] {
        match self {
            Self::Len | Self::Upper | Self::Lower | Self::Trim | Self::Reverse => IN_STR,
            Self::Find | Self::Contains => IN_STR_SUBSTR,
            Self::Left | Self::Right => IN_STR_SIZE,
            Self::Mid | Self::Delete => IN_STR_POS_LEN,
            Self::Concat => IN_STR1_STR2,
            Self::Insert => IN_STR1_STR2_POS,
            Self::Replace => IN_STR1_STR2_POS_LEN,
        }
    }

    /// 输出端口 "result" 的域: Len/Find/Contains 为 F32, 其余为 String
    pub const fn output_domain(&self) -> PortDomain {
        match self {
            Self::Len | Self::Find | Self::Contains => PortDomain::F32,
            _ => PortDomain::String,
        }
    }

    /// 评估字符串操作
    ///
    /// `str_inputs`/`num_inputs` 按端口表 (input_ports) 顺序给全 (调用方保证);
    /// `num_inputs` 传入的已是 "端口连接值或内联默认值" 解析后的最终值,
    /// 本函数不关心来源。缺省防御: 缺失的字符串输入按 "" 处理, 数值按 0 处理。
    #[allow(clippy::cast_precision_loss)]
    pub fn evaluate(&self, str_inputs: &[&str], num_inputs: &[f32]) -> StrResult {
        let s = |i: usize| str_inputs.get(i).copied().unwrap_or("");
        let n = |i: usize| num_inputs.get(i).copied().unwrap_or(0.0);
        match self {
            Self::Len => StrResult::Num(char_len(s(0)) as f32),
            Self::Find => StrResult::Num(
                // 字节索引 → 1-based 字符索引
                s(0).find(s(1)).map_or(0.0, |byte_idx| {
                    (s(0)[..byte_idx].chars().count() + 1) as f32
                }),
            ),
            Self::Contains => StrResult::Num(if s(0).contains(s(1)) { 1.0 } else { 0.0 }),
            Self::Left => {
                let size = to_count(n(0));
                // SIZE=0 → 整串
                StrResult::Text(if size == 0 {
                    s(0).to_owned()
                } else {
                    char_slice(s(0), 0, size)
                })
            }
            Self::Right => {
                let size = to_count(n(0));
                let len = char_len(s(0));
                StrResult::Text(if size == 0 {
                    s(0).to_owned()
                } else {
                    char_slice(s(0), len.saturating_sub(size), len)
                })
            }
            Self::Mid => {
                let src = s(0);
                let len = char_len(src);
                let start = to_count(n(0)).clamp(1, len + 1) - 1;
                let count = to_count(n(1));
                // LEN=0 → 从 POS 到末尾
                let end = if count == 0 { len } else { start + count };
                StrResult::Text(char_slice(src, start, end))
            }
            Self::Concat => {
                let (a, b) = (s(0), s(1));
                StrResult::Text(format!("{a}{b}"))
            }
            Self::Insert => {
                let src = s(0);
                let len = char_len(src);
                // pos=1 → 头部插入, pos=len+1 → 尾部追加
                let start = to_count(n(0)).clamp(1, len + 1) - 1;
                let (head, mid, tail) =
                    (char_slice(src, 0, start), s(1), char_slice(src, start, len));
                StrResult::Text(format!("{head}{mid}{tail}"))
            }
            Self::Delete => {
                let src = s(0);
                let len = char_len(src);
                let pos = to_count(n(0));
                if pos > len {
                    // POS 超出长度 → no-op
                    StrResult::Text(src.to_owned())
                } else {
                    let start = pos.max(1) - 1;
                    let count = to_count(n(1));
                    let end = if count == 0 { len } else { start + count };
                    let (head, tail) = (char_slice(src, 0, start), char_slice(src, end, len));
                    StrResult::Text(format!("{head}{tail}"))
                }
            }
            Self::Replace => {
                let src = s(0);
                let len = char_len(src);
                let pos = to_count(n(0));
                if pos > len {
                    // POS 超出长度 → no-op
                    StrResult::Text(src.to_owned())
                } else {
                    let start = pos.max(1) - 1;
                    let count = to_count(n(1));
                    let end = if count == 0 { len } else { start + count };
                    let (head, mid, tail) =
                        (char_slice(src, 0, start), s(1), char_slice(src, end, len));
                    StrResult::Text(format!("{head}{mid}{tail}"))
                }
            }
            Self::Upper => StrResult::Text(s(0).to_uppercase()),
            Self::Lower => StrResult::Text(s(0).to_lowercase()),
            Self::Trim => StrResult::Text(s(0).trim().to_owned()),
            Self::Reverse => StrResult::Text(s(0).chars().rev().collect()),
        }
    }
}

#[cfg(test)]
// 测试结果均为精确可表示的值 (小整数 f32 / 字面字符串), 直接断言相等
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

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

    // ---- Len ----
    #[test]
    fn test_len() {
        assert_eq!(num(StrOp::Len.evaluate(&["hello"], &[])), 5.0);
        assert_eq!(num(StrOp::Len.evaluate(&[""], &[])), 0.0);
        // 多字节字符按字符计
        assert_eq!(num(StrOp::Len.evaluate(&["你好世界"], &[])), 4.0);
    }

    // ---- Find ----
    #[test]
    fn test_find() {
        assert_eq!(
            num(StrOp::Find.evaluate(&["hello world", "world"], &[])),
            7.0
        );
        // 未命中返回 0
        assert_eq!(num(StrOp::Find.evaluate(&["hello", "xyz"], &[])), 0.0);
        // 多字节: 1-based 字符位置
        assert_eq!(num(StrOp::Find.evaluate(&["你好世界", "世界"], &[])), 3.0);
    }

    // ---- Contains ----
    #[test]
    fn test_contains() {
        assert_eq!(num(StrOp::Contains.evaluate(&["hello", "ell"], &[])), 1.0);
        assert_eq!(num(StrOp::Contains.evaluate(&["hello", "xyz"], &[])), 0.0);
    }

    // ---- Left / Right ----
    #[test]
    fn test_left() {
        assert_eq!(text(StrOp::Left.evaluate(&["hello"], &[3.0])), "hel");
        // SIZE=0 → 整串
        assert_eq!(text(StrOp::Left.evaluate(&["hello"], &[0.0])), "hello");
        // SIZE 越界 → 截取到末尾
        assert_eq!(text(StrOp::Left.evaluate(&["hello"], &[99.0])), "hello");
        assert_eq!(text(StrOp::Left.evaluate(&["你好世界"], &[2.0])), "你好");
    }

    #[test]
    fn test_right() {
        assert_eq!(text(StrOp::Right.evaluate(&["hello"], &[2.0])), "lo");
        assert_eq!(text(StrOp::Right.evaluate(&["hello"], &[0.0])), "hello");
        assert_eq!(text(StrOp::Right.evaluate(&["hello"], &[99.0])), "hello");
        assert_eq!(text(StrOp::Right.evaluate(&["你好世界"], &[2.0])), "世界");
    }

    // ---- Mid ----
    #[test]
    fn test_mid() {
        assert_eq!(
            text(StrOp::Mid.evaluate(&["hello world"], &[7.0, 5.0])),
            "world"
        );
        assert_eq!(text(StrOp::Mid.evaluate(&["hello"], &[2.0, 3.0])), "ell");
        // LEN=0 → 从 POS 到末尾
        assert_eq!(text(StrOp::Mid.evaluate(&["hello"], &[2.0, 0.0])), "ello");
        // POS=0 → clamp 到 1
        assert_eq!(text(StrOp::Mid.evaluate(&["hello"], &[0.0, 2.0])), "he");
        // POS 越界 → clamp 到 len+1 → 空串
        assert_eq!(text(StrOp::Mid.evaluate(&["hello"], &[99.0, 2.0])), "");
        // LEN 越界 → 截取到末尾
        assert_eq!(text(StrOp::Mid.evaluate(&["hello"], &[4.0, 99.0])), "lo");
        // 空串输入
        assert_eq!(text(StrOp::Mid.evaluate(&[""], &[1.0, 0.0])), "");
        // 多字节按字符索引
        assert_eq!(
            text(StrOp::Mid.evaluate(&["你好世界"], &[2.0, 2.0])),
            "好世"
        );
        // 数值 round: pos=2.6 → 3
        assert_eq!(text(StrOp::Mid.evaluate(&["hello"], &[2.6, 1.0])), "l");
    }

    // ---- Concat ----
    #[test]
    fn test_concat() {
        assert_eq!(text(StrOp::Concat.evaluate(&["foo", "bar"], &[])), "foobar");
        assert_eq!(text(StrOp::Concat.evaluate(&["", ""], &[])), "");
    }

    // ---- Insert ----
    #[test]
    fn test_insert() {
        assert_eq!(text(StrOp::Insert.evaluate(&["acd", "b"], &[2.0])), "abcd");
        // POS=1 → 头部插入
        assert_eq!(text(StrOp::Insert.evaluate(&["bc", "a"], &[1.0])), "abc");
        // POS=0 → clamp 到 1
        assert_eq!(text(StrOp::Insert.evaluate(&["bc", "a"], &[0.0])), "abc");
        // POS 越界 → clamp 到 len+1 → 尾部追加
        assert_eq!(text(StrOp::Insert.evaluate(&["ab", "c"], &[99.0])), "abc");
        // 多字节
        assert_eq!(
            text(StrOp::Insert.evaluate(&["你好", "呀"], &[3.0])),
            "你好呀"
        );
    }

    // ---- Delete ----
    #[test]
    fn test_delete() {
        assert_eq!(text(StrOp::Delete.evaluate(&["hello"], &[2.0, 3.0])), "ho");
        // LEN=0 → 从 POS 删到末尾
        assert_eq!(text(StrOp::Delete.evaluate(&["hello"], &[3.0, 0.0])), "he");
        // POS 超出长度 → no-op
        assert_eq!(
            text(StrOp::Delete.evaluate(&["hello"], &[6.0, 1.0])),
            "hello"
        );
        assert_eq!(
            text(StrOp::Delete.evaluate(&["hello"], &[99.0, 1.0])),
            "hello"
        );
        // LEN 越界 → 删到末尾为止
        assert_eq!(text(StrOp::Delete.evaluate(&["hello"], &[2.0, 99.0])), "h");
        // 多字节
        assert_eq!(
            text(StrOp::Delete.evaluate(&["你好世界"], &[2.0, 2.0])),
            "你界"
        );
    }

    // ---- Replace ----
    #[test]
    fn test_replace() {
        assert_eq!(
            text(StrOp::Replace.evaluate(&["hello", "XY"], &[2.0, 3.0])),
            "hXYo"
        );
        // LEN=0 → 替换 POS 到末尾
        assert_eq!(
            text(StrOp::Replace.evaluate(&["hello", "XY"], &[3.0, 0.0])),
            "heXY"
        );
        // POS 超出长度 → no-op 返回原串
        assert_eq!(
            text(StrOp::Replace.evaluate(&["hello", "XY"], &[6.0, 1.0])),
            "hello"
        );
        // LEN 越界 → 替换到末尾
        assert_eq!(
            text(StrOp::Replace.evaluate(&["hello", "XY"], &[4.0, 99.0])),
            "helXY"
        );
        // 多字节
        assert_eq!(
            text(StrOp::Replace.evaluate(&["你好世界", "吧"], &[4.0, 1.0])),
            "你好世吧"
        );
    }

    // ---- Upper / Lower / Trim / Reverse ----
    #[test]
    fn test_case_trim_reverse() {
        assert_eq!(text(StrOp::Upper.evaluate(&["hello"], &[])), "HELLO");
        assert_eq!(text(StrOp::Lower.evaluate(&["HeLLo"], &[])), "hello");
        assert_eq!(text(StrOp::Trim.evaluate(&["  hi \n"], &[])), "hi");
        assert_eq!(text(StrOp::Reverse.evaluate(&["hello"], &[])), "olleh");
        // 多字节反转按字符 (非字节)
        assert_eq!(text(StrOp::Reverse.evaluate(&["你好"], &[])), "好你");
    }

    // ---- StrNumParams ----
    #[test]
    fn test_num_params_default() {
        let p = StrNumParams::default();
        assert_eq!(p.pos, 1.0);
        assert_eq!(p.len, 0.0);
        assert_eq!(p.size, 0.0);
    }

    // ---- 端口表 ----
    #[test]
    fn test_port_tables() {
        assert_eq!(StrOp::Len.input_ports(), &[("str", PortDomain::String)]);
        assert_eq!(
            StrOp::Mid.input_ports(),
            &[
                ("str", PortDomain::String),
                ("pos", PortDomain::F32),
                ("len", PortDomain::F32)
            ]
        );
        assert_eq!(
            StrOp::Replace.input_ports(),
            &[
                ("str1", PortDomain::String),
                ("str2", PortDomain::String),
                ("pos", PortDomain::F32),
                ("len", PortDomain::F32)
            ]
        );
        assert_eq!(StrOp::Len.output_domain(), PortDomain::F32);
        assert_eq!(StrOp::Find.output_domain(), PortDomain::F32);
        assert_eq!(StrOp::Contains.output_domain(), PortDomain::F32);
        assert_eq!(StrOp::Concat.output_domain(), PortDomain::String);
    }

    // ---- serde 命名风格 (与 MathOp 一致: lowercase) ----
    #[test]
    fn test_serde_lowercase() {
        assert_eq!(
            serde_json::to_string(&StrOp::Replace).unwrap(),
            "\"replace\""
        );
        let op: StrOp = serde_json::from_str("\"mid\"").unwrap();
        assert_eq!(op, StrOp::Mid);
    }
}
