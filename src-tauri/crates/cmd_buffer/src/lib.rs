//! `cmd_buffer` — 波形缓冲区 + 窗口视觉效果 Tauri 命令
//!
//! Stage H Task #7 拆分: 由 `src-tauri/src/commands/{buffer.rs, window.rs}` 提取而来。

mod buffer;
mod window;

pub use buffer::*;
pub use window::*;
