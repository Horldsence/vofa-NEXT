//! `rawdata` — 原始数据订阅 + 帧解码器手动解析 Tauri 命令
//!
//! 层级: L4 cmd — Tauri IPC 薄适配; 允许依赖 L0-L3, 禁止被任何非 cmd crate 依赖。

mod frame_decoder;
mod rawdata;

pub use frame_decoder::*;
pub use rawdata::*;
