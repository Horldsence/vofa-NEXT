//! `cmd_pipeline` — 流水线参数 + 触发器匹配 Tauri 命令
//!
//! Stage H Task #7 拆分: 由 `src-tauri/src/commands/{pipeline.rs, trigger.rs}` 提取而来。

mod pipeline;
mod trigger;

pub use pipeline::*;
pub use trigger::*;
