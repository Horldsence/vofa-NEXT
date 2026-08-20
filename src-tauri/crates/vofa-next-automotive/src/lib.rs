//! `vofa-next-automotive` — 诊断协议层 façade (ISO-TP / UDS / OBD-II / J1939)
//!
//! 基于 `libautomotive`,在现有 Slcan / CandleLight 裸 CAN 帧管线之上叠加 OSI
//! 传输/网络/应用层,提供统一的 `DiagnosticEngine` 入口与异步事件流。
//!
//! 实际实现位于三个子 crate:
//! - `automotive_isotp` — ISO 15765-2 (ISO-TP) 传输层 (IsoTpSession 等)
//! - `automotive_can` — Slcan / CandleLight → 统一 `CanBackend` 桥接
//! - `automotive_diag` — `DiagnosticEngine` 入口 (Phase 1 占位,UDS/OBD/J1939 后续接入)
//!
//! 本 crate 仅为兼容 façade,新代码请直接依赖子 crate。

pub use automotive_can::{BackendKind, BridgeCanBackend};
pub use automotive_diag::DiagnosticEngine;
pub use automotive_isotp::{
    AutomotiveError, AutomotiveResult, IsoTpSession, IsoTpSessionHandle,
};
pub use vofa_next_transport::CanBackend;