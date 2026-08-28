//! per-kind lowering 分派 — 每种 NodeKind 一个 lower_* 函数,见 kinds/* 子文件
//! 槽位/输入解析语义与 evaluate_into 慢路径逐臂一致 (equiv_tests 等价性校验背书)
//!
//! 新增节点类型 = 加一个 lower 函数 + `lower_node` match 加一行,不动流水线

use node_kind::{NodeDef, NodeKind};

use crate::lower::LowerCtx;

mod custom;
mod filter;
mod frame_decoder;
mod ifft;
mod input;
mod math;
mod protocol_source;
mod str;
mod text_input;
mod textout;
mod trigger;

use custom::lower_custom;
use filter::lower_filter;
use frame_decoder::lower_frame_decoder;
use ifft::lower_ifft;
use input::lower_input;
use math::lower_math;
use protocol_source::lower_protocol_source;
use str::lower_str;
use text_input::lower_text_input;
use textout::lower_textout;
use trigger::lower_trigger;

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
        NodeKind::Str { op, num, tmpl } => lower_str(node, *op, num, tmpl, ctx),
        // TextOut 参与 eval_order (无输出端口, 透传写自身 "text" 槽位 + 收集发送规格)
        NodeKind::TextOut { .. } => lower_textout(node, ctx),
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
        | NodeKind::Protocol { .. } => {}
    }
}
