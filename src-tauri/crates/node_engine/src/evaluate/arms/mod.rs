//! 求值 arm 分发表 — 按 NodeKind variant 路由到对应 unit struct impl
//! arm.run 接收 (graph, node_id, ctx) 三个参数 — graph 与 node_id 独立于 ctx,
//! 避免 mut borrow ctx.out 与 immut borrow ctx.node.id 的双重借用冲突

use node_kind::NodeKind;

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
pub use trigger::TriggerArm;

/// 按 NodeKind variant 分派到对应 arm;Sink / SpectrumSink / Transport / Protocol
/// 无值平面输出,返回 None 由主循环跳过
pub fn arm_for(kind: &NodeKind) -> Option<Box<dyn NodeArm>> {
    match kind {
        NodeKind::Input => Some(Box::new(InputArm)),
        NodeKind::Math { .. } => Some(Box::new(MathArm)),
        NodeKind::Custom { .. } => Some(Box::new(CustomArm)),
        NodeKind::Filter { .. } => Some(Box::new(FilterArm)),
        NodeKind::FrameDecoder { .. } => Some(Box::new(FrameDecoderArm)),
        NodeKind::Ifft => Some(Box::new(IfftArm)),
        NodeKind::Str { .. } => Some(Box::new(StrArm)),
        NodeKind::Trigger { .. } => Some(Box::new(TriggerArm)),
        NodeKind::ProtocolSource { .. } => Some(Box::new(ProtocolSourceArm)),
        NodeKind::TextInput { .. } => Some(Box::new(TextInputArm)),
        NodeKind::Sink
        | NodeKind::SpectrumSink { .. }
        | NodeKind::Transport { .. }
        | NodeKind::Protocol { .. } => None,
    }
}