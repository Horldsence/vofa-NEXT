//! TextInput lowering — 输出端口固定 "str" → 字符串槽位,参数 text 每帧原样写入

use node_kind::NodeDef;

use crate::lower::LowerCtx;
use crate::ops::CompiledOp;

pub(super) fn lower_text_input(node: &NodeDef, text: &str, ctx: &mut LowerCtx) {
    let out = ctx.str_slots.alloc(&node.id, "str");
    ctx.push_op(CompiledOp::TextInput {
        text: text.to_string(),
        out,
    });
}
