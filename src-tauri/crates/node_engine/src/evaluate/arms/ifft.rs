//! Ifft arm — ifft_states[node_id].next_sample() 写入 "out0"

use node_kind::NodeKind;

use crate::compile::CompiledGraph;
use node_eval::{node_out_entry, set_port};

use super::{EvalCtx, NodeArm};

pub struct IfftArm;

impl NodeArm for IfftArm {
    fn run(&self, graph: &CompiledGraph, node_id: &str, ctx: &mut EvalCtx<'_>) {
        let Some(node) = graph.value_def(node_id) else {
            return;
        };
        if !matches!(node.kind, NodeKind::Ifft) {
            return;
        }
        let v = ctx
            .ifft_states
            .get_mut(node_id)
            .map_or(0.0, dsp_fft::IfftState::next_sample);
        set_port(node_out_entry(ctx.out, node_id), "out0", v);
    }
}
