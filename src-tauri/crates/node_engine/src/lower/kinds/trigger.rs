//! Trigger lowering — value/matched 分配 f32 槽位,text 分配字符串槽位
//! auto 模式的 "trigger" 输入端口经 input_index 解析 (无边 = None → 0.0)

use node_kind::NodeDef;
use node_trigger::TriggerRuleDef;

use crate::lower::LowerCtx;
use crate::ops::CompiledOp;

pub(super) fn lower_trigger(
    node: &NodeDef,
    mode: &str,
    edge: &str,
    default_miss: f32,
    default_miss_text: &str,
    command: &str,
    rules: &[TriggerRuleDef],
    ctx: &mut LowerCtx,
) {
    let trigger_in = ctx.f32_in(&node.id, "trigger");
    let value = ctx.f32_slots.alloc(&node.id, "value");
    let matched = ctx.f32_slots.alloc(&node.id, "matched");
    let text = ctx.str_slots.alloc(&node.id, "text");
    ctx.ops.push(CompiledOp::Trigger {
        node_id: node.id.clone(),
        mode: mode.to_string(),
        edge: edge.to_string(),
        default_miss,
        default_miss_text: default_miss_text.to_string(),
        command: command.to_string(),
        rules: rules.to_vec(),
        trigger_in,
        value,
        matched,
        text,
    });
}