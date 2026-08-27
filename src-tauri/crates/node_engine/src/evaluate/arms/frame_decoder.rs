//! FrameDecoder arm — decoder_states 命中读 outputs + valid/frame_count/last_timestamp/fps;
//! 未命中按 blocks 端口与附加端口默认 0

use node_kind::NodeKind;

use crate::compile::CompiledGraph;
use node_eval::{node_out_entry, set_port};

use super::{EvalCtx, NodeArm};

pub struct FrameDecoderArm;

#[allow(clippy::cast_precision_loss)] // 帧计数/时间戳转 f32 与快路径 CompiledEval 同语义
impl NodeArm for FrameDecoderArm {
    fn run(&self, graph: &CompiledGraph, node_id: &str, ctx: &mut EvalCtx<'_>) {
        let Some(node) = graph.value_def(node_id) else {
            return;
        };
        let NodeKind::FrameDecoder {
            blocks,
            enable_valid,
            enable_frame_count,
            enable_last_timestamp,
            enable_fps,
            loopback: _,
        } = &node.kind
        else {
            return;
        };
        let m = node_out_entry(ctx.out, node_id);
        if let Some(parser) = ctx.decoder_states.get(node_id) {
            for (k, &v) in &parser.last_frame.outputs {
                set_port(m, k, v);
            }
            if *enable_valid {
                set_port(m, "valid", if parser.last_frame.valid { 1.0 } else { 0.0 });
            }
            if *enable_frame_count {
                set_port(m, "frame_count", parser.frame_count as f32);
            }
            if *enable_last_timestamp {
                set_port(m, "last_timestamp", parser.last_frame.timestamp_us as f32);
            }
            if *enable_fps {
                set_port(m, "fps", parser.fps());
            }
        } else {
            for b in blocks {
                if let Some(port) = b.output_port_name() {
                    set_port(m, port, 0.0);
                }
            }
            if *enable_valid {
                set_port(m, "valid", 0.0);
            }
            if *enable_frame_count {
                set_port(m, "frame_count", 0.0);
            }
            if *enable_last_timestamp {
                set_port(m, "last_timestamp", 0.0);
            }
            if *enable_fps {
                set_port(m, "fps", 0.0);
            }
        }
    }
}
