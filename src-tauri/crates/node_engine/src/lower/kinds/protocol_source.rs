//! ProtocolSource lowering — 每通道一个 op;"str" 端口走字符串槽位

use node_kind::NodeDef;

use crate::lower::LowerCtx;
use crate::ops::CompiledOp;

pub(super) fn lower_protocol_source(
    node: &NodeDef,
    source_id: &str,
    channels: usize,
    port_names: Option<&[String]>,
    ctx: &mut LowerCtx,
) {
    let src = ctx.frame_source(source_id);
    let names = node_kind::protocol_source_port_names(port_names, channels);
    for (i, port) in names.iter().enumerate() {
        if port == "str" {
            let slot = ctx.str_slots.alloc(&node.id, port);
            ctx.ops.push(CompiledOp::ProtocolSourceStr { src, slot });
        } else {
            let slot = ctx.f32_slots.alloc(&node.id, port);
            ctx.ops
                .push(CompiledOp::ProtocolSource { src, ch: i, slot });
        }
    }
}