//! Math lowering — 输入经 in_names 端口名反查槽位 (无边 = None → 常量 0.0)

use node_kind::{MathOp, NodeDef};

use crate::lower::LowerCtx;
use crate::ops::CompiledOp;

pub(super) fn lower_math(node: &NodeDef, op: MathOp, input_count: usize, ctx: &mut LowerCtx) {
    let inputs = (0..input_count)
        .map(|i| {
            let in_name = ctx
                .mir
                .in_names
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("in{i}"));
            ctx.f32_in(&node.id, &in_name)
        })
        .collect();
    let out = ctx.f32_slots.alloc(&node.id, "result");
    ctx.push_op(CompiledOp::Math { op, inputs, out });
}
