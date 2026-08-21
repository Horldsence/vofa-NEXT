//! `automotive_isotp` — ISO 15765-2 (ISO-TP) 传输层
//!
//! 在 `transport_core` 的 `CanBackend` 之上,实现 ISO-TP 状态机:
//! - 单帧 (SF) / 首帧 (FF) / 连续帧 (CF) / 流控帧 (FC) 装配与解析
//! - 基于 tokio 的异步会话 (`IsoTpSession` + `IsoTpSessionHandle`)
//! - PCI 常量、block size、STmin 等配置遵循 ISO 15765-2 标准
//!
//! 错误类型 (`AutomotiveError` / `AutomotiveResult`) 在本 crate 定义,
//! 也供 `automotive_diag` 与 `automotive_can` 复用。

pub mod constants;
mod error;
mod rx;
mod session;
mod state;
mod task;
mod tx;

pub use error::{AutomotiveError, AutomotiveResult};
pub use session::{IsoTpSession, IsoTpSessionHandle};