//! Filter arm — resolve_input("in0") + filter_kind_from_config 派生 + 懒重建

use dsp_filter::DigitalFilter;

use crate::compile::CompiledGraph;
use eval::{node_out_entry, set_port};

use super::{EvalCtx, NodeArm};

pub struct FilterArm;

impl NodeArm for FilterArm {
    fn run(&self, graph: &CompiledGraph, node_id: &str, ctx: &mut EvalCtx<'_>) {
        let Some(new_kind) = graph.filter_kinds.get(node_id) else {
            return;
        };
        let input_val = graph.resolve_input(node_id, "in0", ctx.out);
        let need_rebuild = ctx
            .filter_states
            .get(node_id)
            .is_none_or(|f| f.kind() != new_kind);
        if need_rebuild {
            ctx.filter_states
                .insert(node_id.to_string(), DigitalFilter::new(new_kind.clone()));
        }
        let result = ctx
            .filter_states
            .get_mut(node_id)
            .expect("filter 状态已在上方缺失时插入")
            .process(input_val);
        set_port(node_out_entry(ctx.out, node_id), "result", result);
    }
}
