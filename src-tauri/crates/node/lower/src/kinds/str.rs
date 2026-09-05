//! Str lowering — 输入按 StrOp::input_ports() 端口表顺序紧凑拆分 str_inputs/num_inputs
//! (只含同 domain 端口,与 StrOp::evaluate 的紧凑对齐约定一致)
//! 输出端口固定 "result",域由 op.output_domain() 决定
//!
//! 字符串内联回退: 未连接的字符串端口编译期捕获 `str_defaults` (当前仅 FORMAT 的
//! "fmt" 端口 ← `NodeKind::Str.tmpl`; 其余端口/算子为空串), 求值语义对称于 num_defaults。

use kind::{str_num_default, NodeDef, PortDomain, StrNumParams, StrOp};

use crate::lower::LowerCtx;
use crate::ops::CompiledOp;

pub(super) fn lower_str(
    node: &NodeDef,
    op: StrOp,
    num: &StrNumParams,
    tmpl: &str,
    ctx: &mut LowerCtx,
) {
    let mut str_inputs = Vec::new();
    let mut str_defaults = Vec::new();
    let mut num_inputs = Vec::new();
    let mut num_defaults = Vec::new();
    for (name, domain) in op.input_ports() {
        match domain {
            PortDomain::String => {
                str_inputs.push(ctx.str_in(&node.id, name));
                // 内联回退文本: 仅 FORMAT 的 fmt 端口取模板参数, 其余为空串
                str_defaults.push(if op.uses_inline_text_default(name) {
                    tmpl.to_string()
                } else {
                    String::new()
                });
            }
            PortDomain::F32 => {
                num_inputs.push(ctx.f32_in(&node.id, name));
                num_defaults.push(str_num_default(num, name));
            }
            PortDomain::Bytes | PortDomain::Spectrum => {}
        }
    }
    let (text_out, num_out) = match op.output_domain() {
        PortDomain::String => (Some(ctx.str_slots.alloc(&node.id, "result")), None),
        PortDomain::F32 => (None, Some(ctx.f32_slots.alloc(&node.id, "result"))),
        PortDomain::Bytes | PortDomain::Spectrum => (None, None),
    };
    ctx.push_op(CompiledOp::Str {
        op,
        str_inputs,
        str_defaults: str_defaults.into(),
        num_inputs,
        num_defaults,
        text_out,
        num_out,
    });
}
