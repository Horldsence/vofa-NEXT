//! TextOut lowering — "text" 输入槽位透传写 (通用发布通道) + 编译期发送规格收集

use kind::{NodeDef, NodeKind};

use crate::lower::LowerCtx;
use crate::ops::{CompiledOp, TextOutSpec};

pub(super) fn lower_textout(node: &NodeDef, ctx: &mut LowerCtx<'_>) {
    let NodeKind::TextOut {
        target_transport,
        newline,
        min_interval_ms,
    } = &node.kind
    else {
        return;
    };
    ctx.textouts.push(TextOutSpec::from_kind(
        &node.id,
        target_transport,
        *newline,
        *min_interval_ms,
    ));

    // 透传 op: 上游字符串 → 本节点 "text" 槽位 (经 materialize_str 进 graph_string_outputs);
    // 未连接 (input = None) 时不写 — 无值不发
    let input = ctx.str_in(&node.id, "text");
    let out = ctx.str_slots.alloc(&node.id, "text");
    ctx.push_op(CompiledOp::TextOut { input, out });
}
