//! ProtocolSource arm — source_frames / source_texts 双通道
//! "str" 端口走字符串平面 (RawData 原始字节文本),其余按通道下标读数值平面

use node_kind::{protocol_source_port_names, NodeKind};

use crate::compile::CompiledGraph;
use node_eval::{node_out_entry, node_out_str_entry, set_port, set_str_port};

use super::{EvalCtx, NodeArm};

pub struct ProtocolSourceArm;

impl NodeArm for ProtocolSourceArm {
    fn run(&self, graph: &CompiledGraph, node_id: &str, ctx: &mut EvalCtx<'_>) {
        let Some(node) = graph.value_def(node_id) else {
            return;
        };
        let NodeKind::ProtocolSource {
            node_id: source_id,
            channels,
            port_names,
        } = &node.kind
        else {
            return;
        };
        let source_id = source_id.as_str();
        let frame = ctx.source_frames.get(source_id);
        let names = protocol_source_port_names(port_names.as_deref(), *channels);
        for (i, name) in names.iter().enumerate() {
            if name == "str" {
                if let Some(text) = ctx.source_texts.get(source_id) {
                    set_str_port(node_out_str_entry(ctx.out_str, node_id), "str", text);
                }
                continue;
            }
            if let Some(v) = frame.and_then(|f| f.channels.get(i)).copied() {
                set_port(node_out_entry(ctx.out, node_id), name, v);
            }
        }
    }
}
