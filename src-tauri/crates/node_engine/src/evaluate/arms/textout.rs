//! TextOut arm — "text" 输入透传写 out_str[node]["text"] (与快路径 CompiledOp::TextOut 一致)

use node_kind::NodeKind;

use crate::compile::CompiledGraph;
use node_eval::{node_out_str_entry, set_str_port};

use super::{EvalCtx, NodeArm};

pub struct TextOutArm;

impl NodeArm for TextOutArm {
    fn run(&self, graph: &CompiledGraph, node_id: &str, ctx: &mut EvalCtx<'_>) {
        let Some(node) = graph.value_def(node_id) else {
            return;
        };
        let NodeKind::TextOut { .. } = &node.kind else {
            return;
        };
        // 输入端口固定 "text" (String 域); 未连接时不发布 (无值不发)
        if graph
            .string_input_index
            .get(node_id)
            .and_then(|ports| ports.get("text"))
            .is_none()
        {
            return;
        }
        let text = graph
            .resolve_str_input(node_id, "text", ctx.out_str)
            .to_owned();
        set_str_port(node_out_str_entry(ctx.out_str, node_id), "text", &text);
    }
}
