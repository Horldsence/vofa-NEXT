//! FrameDecoder lowering — 端口列表编译期确定 (blocks 的 port_name + 按开关的附加端口)

use kind::{NodeDef, NodeKind};

use crate::lower::LowerCtx;
use crate::ops::CompiledOp;

pub(super) fn lower_frame_decoder(node: &NodeDef, ctx: &mut LowerCtx) {
    let NodeKind::FrameDecoder {
        blocks,
        enable_valid,
        enable_frame_count,
        enable_last_timestamp,
        enable_fps,
        ..
    } = &node.kind
    else {
        return;
    };
    let mut ports = Vec::new();
    for b in blocks {
        if let Some(port) = b.output_port_name() {
            let slot = ctx.f32_slots.alloc(&node.id, port);
            ports.push((port.to_string(), slot));
        }
    }
    let valid = enable_valid.then(|| ctx.f32_slots.alloc(&node.id, "valid"));
    let frame_count = enable_frame_count.then(|| ctx.f32_slots.alloc(&node.id, "frame_count"));
    let last_timestamp =
        enable_last_timestamp.then(|| ctx.f32_slots.alloc(&node.id, "last_timestamp"));
    let fps = enable_fps.then(|| ctx.f32_slots.alloc(&node.id, "fps"));
    ctx.push_op(CompiledOp::FrameDecoder {
        node_id: node.id.clone(),
        ports,
        valid,
        frame_count,
        last_timestamp,
        fps,
    });
}
