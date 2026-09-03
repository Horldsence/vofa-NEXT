//! `pipeline` — 流水线参数 + 触发器匹配 Tauri 命令
//!
//! 层级: L4 cmd — Tauri IPC 薄适配; 允许依赖 L0-L3, 禁止被任何非 cmd crate 依赖。

mod pipeline;
mod trigger;

pub use pipeline::*;
pub use trigger::*;
