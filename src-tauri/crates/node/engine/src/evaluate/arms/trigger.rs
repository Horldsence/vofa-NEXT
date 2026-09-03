//! Trigger arm — matches_config 重建 + auto/manual 分派 + string/number 输出分派
//! manual: record_prev 同步 prev 避免切回 auto+rising 时误触发;auto: eval_auto 边沿检测

use kind::NodeKind;
use trigger::TriggerState;

use crate::compile::CompiledGraph;
use eval::{node_out_entry, node_out_str_entry, set_port, set_str_port};

use super::{EvalCtx, NodeArm};

pub struct TriggerArm;

impl NodeArm for TriggerArm {
    fn run(&self, graph: &CompiledGraph, node_id: &str, ctx: &mut EvalCtx<'_>) {
        let Some(node) = graph.value_def(node_id) else {
            return;
        };
        let NodeKind::Trigger {
            mode,
            edge,
            default_miss,
            default_miss_text,
            command,
            rules,
        } = &node.kind
        else {
            return;
        };
        let need_rebuild = ctx
            .trigger_states
            .get(node_id)
            .is_none_or(|s| !s.matches_config(rules, *default_miss, default_miss_text));
        if need_rebuild {
            ctx.trigger_states.insert(
                node_id.to_string(),
                TriggerState::new(rules.clone(), *default_miss, default_miss_text.clone()),
            );
        }
        let state = ctx.trigger_states.get_mut(node_id).unwrap();
        let tv = graph.resolve_input(node_id, "trigger", ctx.out);
        let result = if mode == "auto" {
            state.eval_auto(edge, tv)
        } else {
            state.record_prev(tv);
            Some(state.eval_manual(command))
        };
        if let Some(r) = result {
            let m = node_out_entry(ctx.out, node_id);
            if r.output_type == "string" {
                set_str_port(node_out_str_entry(ctx.out_str, node_id), "text", &r.text);
            } else {
                set_port(m, "value", r.value);
            }
            set_port(m, "matched", if r.matched { 1.0 } else { 0.0 });
        }
    }
}
