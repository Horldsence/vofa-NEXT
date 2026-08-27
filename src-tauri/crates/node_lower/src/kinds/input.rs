//! Input lowering — input_values[node_id] → "value" 数值槽位

use node_kind::NodeDef;

use crate::lower::LowerCtx;
use crate::ops::CompiledOp;

pub(super) fn lower_input(node: &NodeDef, ctx: &mut LowerCtx) {
    let slot = ctx.f32_slots.alloc(&node.id, "value");
    ctx.ops.push(CompiledOp::Input {
        node_id: node.id.clone(),
        slot,
    });
}
