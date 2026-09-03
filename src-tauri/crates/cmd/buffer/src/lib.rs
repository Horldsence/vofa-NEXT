//! `buffer` — 波形缓冲区 + 窗口视觉效果 Tauri 命令
//!
//! 层级: L4 cmd — Tauri IPC 薄适配; 允许依赖 L0-L3, 禁止被任何非 cmd crate 依赖。

mod buffer;
mod command_frame;
mod frame_checksum;
mod frame_field;
mod window;

pub use buffer::*;
pub use command_frame::*;
pub use frame_checksum::*;
pub use frame_field::*;
pub use window::*;
