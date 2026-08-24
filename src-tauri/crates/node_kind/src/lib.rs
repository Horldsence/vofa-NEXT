//! # node_kind
//!
//! VOFA-NEXT 节点种类系统 — 节点定义 + 端口域模型 + Math 运算 + DecoderBlockDef 转发.
//!
//! 模块（按职责拆分，原 505 行的 `node_kind.rs` 现拆为四层）：
//! - [`NodeKind`][] 节点种类枚举（`node_kind.rs`）
//! - [`NodeDef`][] 节点定义（`spec.rs`）
//! - [`PortDomain`][crate::ports::PortDomain] / 端口 handle 常量（`ports.rs`）
//! - [`MathOp`][] / [`StrOp`][] / [`StrNumParams`][] / [`StrResult`][]（`math_op.rs` / `str_op`）
//! - [`DecoderBlockDef`][] FrameDecoder 块定义 re-export（`decoder_block.rs`）
//!
//! serde 约定：`NodeKind` 是 `#[serde(tag = "kind", content = "params")]`，前端 TS 镜像见
//! `src/lib/utils/nodeDef.ts`.

mod decoder_block;
mod math_op;
mod node_kind;
mod ports;
mod spec;
mod str_op;

pub use decoder_block::{
    AsciiBase, DecoderBlockDef, DecoderChecksumCover, DecoderChecksumPosition, FieldType,
    LengthUnit,
};
pub use math_op::MathOp;
pub use node_kind::{is_protocol_source, is_raw_data_handle, NodeKind};
pub use ports::{
    port_domain, PortDomain, FRAME_DECODER_IN_HANDLE, LOOPBACK_IN_HANDLE, LOOPBACK_OUT_HANDLE,
    PROTOCOL_IN_HANDLE, PROTOCOL_OUT_HANDLE, RAW_DATA_PORT_PREFIX, TRANSPORT_RX_HANDLE,
    TRANSPORT_TX_HANDLE,
};
pub use spec::{protocol_source_port_names, NodeDef};
pub use str_op::{StrNumParams, StrOp, StrResult};
