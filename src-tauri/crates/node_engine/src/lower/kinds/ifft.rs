//! Ifft lowering — "out0" 槽位 (时域重建采样环形播放)

use node_kind::NodeDef;

use crate::lower::LowerCtx;
use crate::ops::CompiledOp;

pub(super) fn lower_ifft(node: &NodeDef, ctx: &mut LowerCtx) {
    let out = ctx.f32_slots.alloc(&node.id, "out0");
    ctx.ops.push(CompiledOp::Ifft {
        node_id: node.id.clone(),
        out,
    });
}