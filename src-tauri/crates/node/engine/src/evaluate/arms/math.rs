//! Math arm — in_names[i] 上游槽位 → op.evaluate(inputs) → "result"
//! 16 路以内走栈数组避免堆分配 (700k 帧/s 热路径),超出走堆兜底

use kind::NodeKind;

use crate::compile::CompiledGraph;
use eval::{node_out_entry, set_port};

use super::{EvalCtx, NodeArm};

pub struct MathArm;

impl NodeArm for MathArm {
    fn run(&self, graph: &CompiledGraph, node_id: &str, ctx: &mut EvalCtx<'_>) {
        let Some(node) = graph.value_def(node_id) else {
            return;
        };
        let NodeKind::Math { op, input_count } = &node.kind else {
            return;
        };
        let mut stack_buf = [0.0f32; 16];
        let mut heap_buf;
        let inputs: &mut [f32] = if *input_count <= 16 {
            &mut stack_buf[..*input_count]
        } else {
            heap_buf = vec![0.0; *input_count];
            &mut heap_buf
        };
        for (i, slot) in inputs.iter_mut().enumerate() {
            *slot = graph.resolve_input(node_id, &graph.in_names[i], ctx.out);
        }
        let result = op.evaluate(inputs);
        set_port(node_out_entry(ctx.out, node_id), "result", result);
    }
}
