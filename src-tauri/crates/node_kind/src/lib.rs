//! # node_kind
//!
//! VOFA-NEXT 节点种类系统 — 节点定义 + 端口域模型 + Math 运算 + DecoderBlockDef 转发。
//!
//! 图分为两个平面:
//! - **字节平面** (全局): Transport / Protocol / FrameDecoder 字节入口 /
//!   widget 的 loopbackOut 字节出口, 边携带 `Vec<u8>`, 事件驱动
//! - **数值平面** (每 tab 一张图): f32 槽位模型, ProtocolSource 引用全局
//!   Protocol 节点的最新帧 (source_frames)
//!
//! serde 约定: `NodeKind` 为 `#[serde(tag = "kind", content = "params")]`,
//! 前端 TS 镜像见 src/lib/utils/nodeDef.ts。
//!
//! 模块:
//! - [`NodeKind`][]: 节点种类 (Transport/Protocol/ProtocolSource/Input/Math/Custom/
//!   Filter/SpectrumSink/Ifft/FrameDecoder/Sink)
//! - [`NodeDef`][]: 节点定义 (含 id/tab_id/kind)
//! - [`PortDomain`]: 端口域 (Bytes / F32) — 用于边分类
//! - [`MathOp`][]: 算术运算种类
//! - [`StrOp`][]: 字符串操作种类 (含 [`StrNumParams`] 内联数值参数与 [`StrResult`] 结果)
//! - [`DecoderBlockDef`]: FrameDecoder 块定义 (re-export from schema_types)

mod decoder_block;
mod math_op;
mod node_kind;
mod str_op;

pub use decoder_block::{
    AsciiBase, DecoderBlockDef, DecoderChecksumCover, DecoderChecksumPosition, FieldType,
    LengthUnit,
};
pub use math_op::MathOp;
pub use node_kind::{
    port_domain, protocol_source_port_names, NodeDef, NodeKind, PortDomain,
    FRAME_DECODER_IN_HANDLE, LOOPBACK_IN_HANDLE, LOOPBACK_OUT_HANDLE, PROTOCOL_IN_HANDLE,
    PROTOCOL_OUT_HANDLE, RAW_DATA_PORT_PREFIX, TRANSPORT_RX_HANDLE, TRANSPORT_TX_HANDLE,
};
pub use str_op::{StrNumParams, StrOp, StrResult};
