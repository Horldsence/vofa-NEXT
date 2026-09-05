//! 求值 arm 分发表 — 按 NodeKind variant 路由到对应 unit struct impl
//! arm.run 接收 (graph, node_id, ctx) 三个参数 — graph 与 node_id 独立于 ctx,
//! 避免 mut borrow ctx.out 与 immut borrow ctx.node.id 的双重借用冲突

use kind::NodeKind;

use super::{EvalCtx, NodeArm};

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

pub use custom::CustomArm;
pub use filter::FilterArm;
pub use frame_decoder::FrameDecoderArm;
pub use ifft::IfftArm;
pub use input::InputArm;
pub use math::MathArm;
pub use protocol_source::ProtocolSourceArm;
pub use str::StrArm;
pub use text_input::TextInputArm;
pub use textout::TextOutArm;
pub use trigger::TriggerArm;

/// 按 NodeKind variant 分派到对应 arm;Sink / Fft / Transport / Protocol
/// 无值平面输出,返回 None 由主循环跳过 (TextOut 参与求值序: 透传写自身槽位)
pub fn arm_for(kind: &NodeKind) -> Option<&'static dyn NodeArm> {
    match kind {
        NodeKind::Input => Some(&InputArm),
        NodeKind::Math { .. } => Some(&MathArm),
        NodeKind::Custom { .. } => Some(&CustomArm),
        NodeKind::Filter { .. } => Some(&FilterArm),
        NodeKind::FrameDecoder { .. } => Some(&FrameDecoderArm),
        NodeKind::Ifft => Some(&IfftArm),
        NodeKind::Str { .. } => Some(&StrArm),
        NodeKind::Trigger { .. } => Some(&TriggerArm),
        NodeKind::ProtocolSource { .. } => Some(&ProtocolSourceArm),
        NodeKind::TextInput { .. } => Some(&TextInputArm),
        NodeKind::TextOut { .. } => Some(&TextOutArm),
        NodeKind::Sink
        | NodeKind::Fft { .. }
        | NodeKind::Transport { .. }
        | NodeKind::Protocol { .. } => None,
    }
}
