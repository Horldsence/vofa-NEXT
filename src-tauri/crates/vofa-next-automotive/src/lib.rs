//! vofa-next-automotive — 诊断协议层 (ISO-TP / UDS / OBD-II / J1939)
//!
//! 基于 libautomotive,在现有 Slcan / CandleLight 裸 CAN 帧管线之上叠加 OSI
//! 传输/网络/应用层,提供统一的 DiagnosticEngine 入口与异步事件流。
//!
//! 详见项目根目录 `plan.md`。

pub mod can_backend;
pub mod engine;
pub mod error;

pub use can_backend::{BackendKind, BridgeCanBackend};
pub use engine::DiagnosticEngine;
pub use error::{AutomotiveError, AutomotiveResult};
