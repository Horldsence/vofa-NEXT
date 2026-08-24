//! Filter lowering — 读 "in0" 上游槽位 → 滤波器状态 → "result" 槽位
//! filter_states 按 FilterConfig 比较变更重建;运行期由 filter_kind_from_config 派生

use dsp_filter::FilterConfig;
use node_kind::NodeDef;

use crate::lower::LowerCtx;
use crate::ops::CompiledOp;

pub(super) fn lower_filter(node: &NodeDef, config: &FilterConfig, ctx: &mut LowerCtx) {
    let input = ctx.f32_in(&node.id, "in0");
    let out = ctx.f32_slots.alloc(&node.id, "result");
    ctx.ops.push(CompiledOp::Filter {
        node_id: node.id.clone(),
        config: config.clone(),
        input,
        out,
    });
}