//! 字符串操作 — `StrOp` 枚举 + `StrResult` + `StrNumParams` + 评估实现.
//!
//! 模块拆分（评估逻辑下沉到子文件以保持 ≤ 500 行）：
//! - 端口表（单一事实源）：[`input_ports`]
//! - 评估实现：[`eval`]
//!
//! 语义规范:
//! - 索引 1-based（POS 从 1 开始；FIND 命中返回 1-based 位置，未找到返回 0）
//! - 数值参数 `round()` 后 `clamp` 到 `>= 0`；POS `clamp` 到 `[1, len+1]`
//! - LEN/SIZE = 0 表示 "到末尾/全部"（Left/Right SIZE=0 → 整串；
//!   Mid/Delete/Replace LEN=0 → 从 POS 到末尾）
//! - 越界/空输入不报错：截取越界返回可用部分或空串；
//!   DELETE/REPLACE 的 POS 超出长度时为 no-op（返回原串）
//! - 字符串索引按 `chars()` 字符计，不用字节索引（多字节字符安全）

mod eval;
mod input_ports;

use serde::{Deserialize, Serialize};

use crate::ports::PortDomain;

pub use input_ports::{input_ports_for, output_domain_for};

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

/// 数值端口（pos/len/size）的内联默认值
///
/// 端口未连接时求值使用此处的回退值；端口已连接时忽略。各 op 只用与自己相关的字段。
/// 默认值：pos = 1（1-based 起点），len/size = 0（到末尾/全部）
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

/// Str 数值端口的内联回退值 (端口未连接时使用): 端口名 → [`StrNumParams`] 字段。
/// 快/慢两条求值路径共享 (lowering 编译期捕获 + 慢路径逐帧回退)。
pub fn str_num_default(num: &StrNumParams, port: &str) -> f32 {
    match port {
        "pos" => num.pos,
        "len" => num.len,
        "size" => num.size,
        _ => 0.0,
    }
}

impl StrOp {
    /// 输入端口表（按固定顺序 — 求值的 `str_inputs` / `num_inputs` 依此取参）
    pub const fn input_ports(&self) -> &'static [(&'static str, PortDomain)] {
        input_ports_for(*self)
    }

    /// 输出端口 "result" 的域：`Len`/`Find`/`Contains` = F32，其余 = String
    pub const fn output_domain(&self) -> PortDomain {
        output_domain_for(*self)
    }

    /// 字符串操作评估 — 输入顺序与 `input_ports()` 一致
    ///
    /// 缺省防御：缺失的字符串输入按 `""` 处理，数值按 `0.0` 处理。
    pub fn evaluate(&self, str_inputs: &[&str], num_inputs: &[f32]) -> StrResult {
        eval::evaluate(*self, str_inputs, num_inputs)
    }
}

#[cfg(test)]
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

    /// 数值断言统一入口 — 评估输出是整数映射出的精确 f32, 单点放宽浮点严格相等
    #[allow(clippy::float_cmp)]
    fn assert_num(actual: f32, expected: f32) {
        assert_eq!(actual, expected);
    }

    #[test]
    fn len_counts_chars() {
        assert_num(num(StrOp::Len.evaluate(&["hello"], &[])), 5.0);
        assert_num(num(StrOp::Len.evaluate(&[""], &[])), 0.0);
        assert_num(num(StrOp::Len.evaluate(&["你好世界"], &[])), 4.0);
    }

    #[test]
    fn find_is_one_based_char_index() {
        assert_num(
            num(StrOp::Find.evaluate(&["hello world", "world"], &[])),
            7.0,
        );
        assert_num(num(StrOp::Find.evaluate(&["hello", "xyz"], &[])), 0.0);
        assert_num(num(StrOp::Find.evaluate(&["你好世界", "世界"], &[])), 3.0);
    }

    #[test]
    fn contains_returns_one_or_zero() {
        assert_num(num(StrOp::Contains.evaluate(&["hello", "ell"], &[])), 1.0);
        assert_num(num(StrOp::Contains.evaluate(&["hello", "xyz"], &[])), 0.0);
    }

    #[test]
    fn left_right_zero_means_whole_string() {
        assert_eq!(text(StrOp::Left.evaluate(&["hello"], &[3.0])), "hel");
        assert_eq!(text(StrOp::Left.evaluate(&["hello"], &[0.0])), "hello");
        assert_eq!(text(StrOp::Left.evaluate(&["hello"], &[99.0])), "hello");
        assert_eq!(text(StrOp::Left.evaluate(&["你好世界"], &[2.0])), "你好");
        assert_eq!(text(StrOp::Right.evaluate(&["hello"], &[2.0])), "lo");
        assert_eq!(text(StrOp::Right.evaluate(&["hello"], &[0.0])), "hello");
        assert_eq!(text(StrOp::Right.evaluate(&["hello"], &[99.0])), "hello");
        assert_eq!(text(StrOp::Right.evaluate(&["你好世界"], &[2.0])), "世界");
    }

    #[test]
    fn mid_clamps_and_supports_zero_len() {
        assert_eq!(
            text(StrOp::Mid.evaluate(&["hello world"], &[7.0, 5.0])),
            "world"
        );
        assert_eq!(text(StrOp::Mid.evaluate(&["hello"], &[2.0, 3.0])), "ell");
        assert_eq!(text(StrOp::Mid.evaluate(&["hello"], &[2.0, 0.0])), "ello");
        assert_eq!(text(StrOp::Mid.evaluate(&["hello"], &[0.0, 2.0])), "he");
        assert_eq!(text(StrOp::Mid.evaluate(&["hello"], &[99.0, 2.0])), "");
        assert_eq!(text(StrOp::Mid.evaluate(&["hello"], &[4.0, 99.0])), "lo");
        assert_eq!(text(StrOp::Mid.evaluate(&[""], &[1.0, 0.0])), "");
        assert_eq!(
            text(StrOp::Mid.evaluate(&["你好世界"], &[2.0, 2.0])),
            "好世"
        );
        assert_eq!(text(StrOp::Mid.evaluate(&["hello"], &[2.6, 1.0])), "l");
    }

    #[test]
    fn concat_joins_two_strings() {
        assert_eq!(text(StrOp::Concat.evaluate(&["foo", "bar"], &[])), "foobar");
        assert_eq!(text(StrOp::Concat.evaluate(&["", ""], &[])), "");
    }

    #[test]
    fn insert_clamps_position() {
        assert_eq!(text(StrOp::Insert.evaluate(&["acd", "b"], &[2.0])), "abcd");
        assert_eq!(text(StrOp::Insert.evaluate(&["bc", "a"], &[1.0])), "abc");
        assert_eq!(text(StrOp::Insert.evaluate(&["bc", "a"], &[0.0])), "abc");
        assert_eq!(text(StrOp::Insert.evaluate(&["ab", "c"], &[99.0])), "abc");
        assert_eq!(
            text(StrOp::Insert.evaluate(&["你好", "呀"], &[3.0])),
            "你好呀"
        );
    }

    #[test]
    fn delete_zero_len_means_to_end() {
        assert_eq!(text(StrOp::Delete.evaluate(&["hello"], &[2.0, 3.0])), "ho");
        assert_eq!(text(StrOp::Delete.evaluate(&["hello"], &[3.0, 0.0])), "he");
        assert_eq!(
            text(StrOp::Delete.evaluate(&["hello"], &[6.0, 1.0])),
            "hello"
        );
        assert_eq!(
            text(StrOp::Delete.evaluate(&["hello"], &[99.0, 1.0])),
            "hello"
        );
        assert_eq!(text(StrOp::Delete.evaluate(&["hello"], &[2.0, 99.0])), "h");
        assert_eq!(
            text(StrOp::Delete.evaluate(&["你好世界"], &[2.0, 2.0])),
            "你界"
        );
    }

    #[test]
    fn replace_zero_len_means_to_end() {
        assert_eq!(
            text(StrOp::Replace.evaluate(&["hello", "XY"], &[2.0, 3.0])),
            "hXYo"
        );
        assert_eq!(
            text(StrOp::Replace.evaluate(&["hello", "XY"], &[3.0, 0.0])),
            "heXY"
        );
        assert_eq!(
            text(StrOp::Replace.evaluate(&["hello", "XY"], &[6.0, 1.0])),
            "hello"
        );
        assert_eq!(
            text(StrOp::Replace.evaluate(&["hello", "XY"], &[4.0, 99.0])),
            "helXY"
        );
        assert_eq!(
            text(StrOp::Replace.evaluate(&["你好世界", "吧"], &[4.0, 1.0])),
            "你好世吧"
        );
    }

    #[test]
    fn upper_lower_trim_reverse() {
        assert_eq!(text(StrOp::Upper.evaluate(&["hello"], &[])), "HELLO");
        assert_eq!(text(StrOp::Lower.evaluate(&["HeLLo"], &[])), "hello");
        assert_eq!(text(StrOp::Trim.evaluate(&["  hi \n"], &[])), "hi");
        assert_eq!(text(StrOp::Reverse.evaluate(&["hello"], &[])), "olleh");
        assert_eq!(text(StrOp::Reverse.evaluate(&["你好"], &[])), "好你");
    }

    #[test]
    fn huge_len_does_not_overflow() {
        let huge = 1e20_f32;
        assert_eq!(text(StrOp::Mid.evaluate(&["hello"], &[2.0, huge])), "ello");
        assert_eq!(text(StrOp::Delete.evaluate(&["hello"], &[2.0, huge])), "h");
        assert_eq!(
            text(StrOp::Replace.evaluate(&["hello", "XY"], &[2.0, huge])),
            "hXY"
        );
    }

    #[test]
    fn num_params_default() {
        let p = StrNumParams::default();
        assert_num(p.pos, 1.0);
        assert_num(p.len, 0.0);
        assert_num(p.size, 0.0);
    }

    #[test]
    fn port_tables_match_constants() {
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

    #[test]
    fn serde_lowercase() {
        assert_eq!(
            serde_json::to_string(&StrOp::Replace).unwrap(),
            "\"replace\""
        );
        let op: StrOp = serde_json::from_str("\"mid\"").unwrap();
        assert_eq!(op, StrOp::Mid);
    }
}
