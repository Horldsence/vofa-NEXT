//! Input arm — input_values[node_id] → "value" 数值槽位 (缺省 0.0)

use crate::compile::CompiledGraph;
use crate::eval::{node_out_entry, set_port};

use super::{EvalCtx, NodeArm};

pub struct InputArm;

impl NodeArm for InputArm {
    fn run(&self, _graph: &CompiledGraph, node_id: &str, ctx: &mut EvalCtx<'_>) {
        let v = ctx.input_values.get(node_id).copied().unwrap_or(0.0);
        set_port(node_out_entry(ctx.out, node_id), "value", v);
    }
}