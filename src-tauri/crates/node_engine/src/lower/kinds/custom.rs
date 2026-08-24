//! Custom lowering — 各输出端口槽位 (值来自前端 iframe 回传)

use node_kind::NodeDef;

use crate::lower::LowerCtx;
use crate::ops::CompiledOp;

pub(super) fn lower_custom(node: &NodeDef, outputs: &[String], ctx: &mut LowerCtx) {
    let ports = outputs
        .iter()
        .map(|p| (p.clone(), ctx.f32_slots.alloc(&node.id, p)))
        .collect();
    ctx.ops.push(CompiledOp::Custom {
        node_id: node.id.clone(),
        ports,
    });
}