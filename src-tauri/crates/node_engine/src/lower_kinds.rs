//! per-kind lowering — 每种 [`NodeKind`] 一个 `lower_*` 函数
//!
//! 新增节点类型 = 加一个 lower 函数 + [`lower_node`] 分派加一行, 不动流水线。
//! 槽位/输入解析语义与 evaluate_into 慢路径逐臂一致 (equiv_tests 等价性校验背书)。

use node_kind::{NodeDef, NodeKind, PortDomain};

use crate::eval::str_num_default;
use crate::lower::LowerCtx;
use crate::ops::CompiledOp;

/// 按节点 kind 分派 lowering (输入: 值平面拓扑序中的节点)
pub fn lower_node(node: &NodeDef, ctx: &mut LowerCtx) {
    match &node.kind {
        NodeKind::ProtocolSource {
            node_id: source_id,
            channels,
            port_names,
        } => lower_protocol_source(node, source_id, *channels, port_names.as_deref(), ctx),
        NodeKind::Input => lower_input(node, ctx),
        NodeKind::Math { op, input_count } => lower_math(node, *op, *input_count, ctx),
        NodeKind::Custom { outputs, .. } => lower_custom(node, outputs, ctx),
        NodeKind::Filter { config } => lower_filter(node, config, ctx),
        NodeKind::FrameDecoder { .. } => lower_frame_decoder(node, ctx),
        NodeKind::Ifft => lower_ifft(node, ctx),
        NodeKind::Str { op, num } => lower_str(node, *op, num, ctx),
        NodeKind::Trigger {
            mode,
            edge,
            default_miss,
            default_miss_text,
            command,
            rules,
        } => lower_trigger(
            node,
            mode,
            edge,
            *default_miss,
            default_miss_text,
            command,
            rules,
            ctx,
        ),
        NodeKind::TextInput { text } => lower_text_input(node, text, ctx),
        NodeKind::Sink
        | NodeKind::SpectrumSink { .. }
        | NodeKind::Transport { .. }
        | NodeKind::Protocol { .. } => {
            // 无值平面输出的节点不应出现在 eval_order 中, 防御性跳过
        }
    }
}

/// ProtocolSource: 每通道一个 op; "str" 端口 (String 域, RawData 原始字节文本)
/// → 字符串槽位, 不占数值槽位 (与 port_domain 的域划分一致)
fn lower_protocol_source(
    node: &NodeDef,
    source_id: &str,
    channels: usize,
    port_names: Option<&[String]>,
    ctx: &mut LowerCtx,
) {
    let src = ctx.frame_source(source_id);
    let names = node_kind::protocol_source_port_names(port_names, channels);
    for (i, port) in names.iter().enumerate() {
        if port == "str" {
            let slot = ctx.str_slots.alloc(&node.id, port);
            ctx.ops.push(CompiledOp::ProtocolSourceStr { src, slot });
        } else {
            let slot = ctx.f32_slots.alloc(&node.id, port);
            ctx.ops
                .push(CompiledOp::ProtocolSource { src, ch: i, slot });
        }
    }
}

/// Input: input_values[node_id] → "value" 槽位
fn lower_input(node: &NodeDef, ctx: &mut LowerCtx) {
    let slot = ctx.f32_slots.alloc(&node.id, "value");
    ctx.ops.push(CompiledOp::Input {
        node_id: node.id.clone(),
        slot,
    });
}

/// Math: 输入经 in_names 端口名反查槽位 (无边 = None → 常量 0.0)
fn lower_math(node: &NodeDef, op: node_kind::MathOp, input_count: usize, ctx: &mut LowerCtx) {
    let inputs = (0..input_count)
        .map(|i| {
            let in_name = ctx
                .mir
                .in_names
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("in{i}"));
            ctx.f32_in(&node.id, &in_name)
        })
        .collect();
    let out = ctx.f32_slots.alloc(&node.id, "result");
    ctx.ops.push(CompiledOp::Math { op, inputs, out });
}

/// Custom: 各输出端口槽位 (值来自前端 iframe 回传)
fn lower_custom(node: &NodeDef, outputs: &[String], ctx: &mut LowerCtx) {
    let ports = outputs
        .iter()
        .map(|p| (p.clone(), ctx.f32_slots.alloc(&node.id, p)))
        .collect();
    ctx.ops.push(CompiledOp::Custom {
        node_id: node.id.clone(),
        ports,
    });
}

/// Filter: 读 "in0" 上游槽位 → 滤波器状态 → "result" 槽位
///
/// filter_states 按 FilterConfig 比较变更重建; 运行期由
/// `filter_kind_from_config` 派生 FilterKind (b/a 不经 IPC 流转)。
fn lower_filter(node: &NodeDef, config: &dsp_filter::FilterConfig, ctx: &mut LowerCtx) {
    let input = ctx.f32_in(&node.id, "in0");
    let out = ctx.f32_slots.alloc(&node.id, "result");
    ctx.ops.push(CompiledOp::Filter {
        node_id: node.id.clone(),
        config: config.clone(),
        input,
        out,
    });
}

/// FrameDecoder: 端口列表编译期确定 (blocks 的 port_name + 按开关的附加端口)
fn lower_frame_decoder(node: &NodeDef, ctx: &mut LowerCtx) {
    let NodeKind::FrameDecoder {
        blocks,
        enable_valid,
        enable_frame_count,
        enable_last_timestamp,
        enable_fps,
        ..
    } = &node.kind
    else {
        return; // 防御: 分派处已匹配
    };
    let mut ports = Vec::new();
    for b in blocks {
        if let Some(port) = b.output_port_name() {
            let slot = ctx.f32_slots.alloc(&node.id, port);
            ports.push((port.to_string(), slot));
        }
    }
    let valid = enable_valid.then(|| ctx.f32_slots.alloc(&node.id, "valid"));
    let frame_count = enable_frame_count.then(|| ctx.f32_slots.alloc(&node.id, "frame_count"));
    let last_timestamp =
        enable_last_timestamp.then(|| ctx.f32_slots.alloc(&node.id, "last_timestamp"));
    let fps = enable_fps.then(|| ctx.f32_slots.alloc(&node.id, "fps"));
    ctx.ops.push(CompiledOp::FrameDecoder {
        node_id: node.id.clone(),
        ports,
        valid,
        frame_count,
        last_timestamp,
        fps,
    });
}

/// Ifft: "out0" 槽位 (时域重建采样环形播放)
fn lower_ifft(node: &NodeDef, ctx: &mut LowerCtx) {
    let out = ctx.f32_slots.alloc(&node.id, "out0");
    ctx.ops.push(CompiledOp::Ifft {
        node_id: node.id.clone(),
        out,
    });
}

/// Str: 输入按 StrOp::input_ports() 端口表顺序紧凑拆分为 str_inputs/num_inputs
/// (只含同 domain 端口, 与 StrOp::evaluate 的紧凑对齐约定一致):
/// - String 端口 → str_inputs: 上游字符串槽位 (未连接 = None ↔ 缺省 "")
/// - F32 端口 → num_inputs (无边 = None) + num_defaults (编译期从 num 捕获的内联回退值)
///
/// 输出端口固定 "result", 域由 op 决定: String → 字符串槽位, F32 → 数值槽位
fn lower_str(
    node: &NodeDef,
    op: node_kind::StrOp,
    num: &node_kind::StrNumParams,
    ctx: &mut LowerCtx,
) {
    let mut str_inputs = Vec::new();
    let mut num_inputs = Vec::new();
    let mut num_defaults = Vec::new();
    for (name, domain) in op.input_ports() {
        match domain {
            PortDomain::String => str_inputs.push(ctx.str_in(&node.id, name)),
            PortDomain::F32 => {
                num_inputs.push(ctx.f32_in(&node.id, name));
                num_defaults.push(str_num_default(num, name));
            }
            PortDomain::Bytes => {} // Str 端口表无 Bytes, 防御
        }
    }
    let (text_out, num_out) = match op.output_domain() {
        PortDomain::String => (Some(ctx.str_slots.alloc(&node.id, "result")), None),
        PortDomain::F32 => (None, Some(ctx.f32_slots.alloc(&node.id, "result"))),
        PortDomain::Bytes => (None, None), // output_domain 无 Bytes, 防御
    };
    ctx.ops.push(CompiledOp::Str {
        op,
        str_inputs,
        num_inputs,
        num_defaults,
        text_out,
        num_out,
    });
}

/// Trigger: value/matched 分配 f32 槽位, text 分配字符串槽位
/// (Trigger.text 由此可被 Str 字符串输入解析);
/// auto 模式的 "trigger" 输入端口经 input_index 解析 (无边 = None → 0.0)
fn lower_trigger(
    node: &NodeDef,
    mode: &str,
    edge: &str,
    default_miss: f32,
    default_miss_text: &str,
    command: &str,
    rules: &[node_trigger::TriggerRuleDef],
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

/// TextInput: 输出端口固定 "str" → 字符串槽位, 参数 text 每帧原样写入
fn lower_text_input(node: &NodeDef, text: &str, ctx: &mut LowerCtx) {
    let out = ctx.str_slots.alloc(&node.id, "str");
    ctx.ops.push(CompiledOp::TextInput {
        text: text.to_string(),
        out,
    });
}
