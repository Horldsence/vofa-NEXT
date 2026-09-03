//! TextInput arm — 参数 text 每帧原样写入 "str" 字符串槽位

use kind::NodeKind;

use crate::compile::CompiledGraph;
use eval::{node_out_str_entry, set_str_port};

use super::{EvalCtx, NodeArm};

pub struct TextInputArm;

impl NodeArm for TextInputArm {
    fn run(&self, graph: &CompiledGraph, node_id: &str, ctx: &mut EvalCtx<'_>) {
        let Some(node) = graph.value_def(node_id) else {
            return;
        };
        if let NodeKind::TextInput { text } = &node.kind {
            set_str_port(node_out_str_entry(ctx.out_str, node_id), "str", text);
        }
    }
}
