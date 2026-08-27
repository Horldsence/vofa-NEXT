//! Custom arm — custom_outputs 回写,缺失则 outputs 默认 0

use node_kind::NodeKind;

use crate::compile::CompiledGraph;
use node_eval::{node_out_entry, set_port};

use super::{EvalCtx, NodeArm};

pub struct CustomArm;

impl NodeArm for CustomArm {
    fn run(&self, graph: &CompiledGraph, node_id: &str, ctx: &mut EvalCtx<'_>) {
        let Some(node) = graph.value_def(node_id) else {
            return;
        };
        let NodeKind::Custom { outputs, .. } = &node.kind else {
            return;
        };
        let m = node_out_entry(ctx.out, node_id);
        if let Some(vals) = ctx.custom_outputs.get(node_id) {
            for (k, &v) in vals {
                set_port(m, k, v);
            }
        } else {
            for p in outputs {
                set_port(m, p, 0.0);
            }
        }
    }
}
