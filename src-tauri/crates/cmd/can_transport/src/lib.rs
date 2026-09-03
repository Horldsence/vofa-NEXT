//! `can_transport` — CAN 帧 / 传输 / 协议 Tauri 命令
//!
//! 层级: L4 cmd — Tauri IPC 薄适配; 允许依赖 L0-L3, 禁止被任何非 cmd crate 依赖。

mod can;
mod protocol;
mod transport;

pub use can::*;
pub use protocol::*;
pub use transport::*;
