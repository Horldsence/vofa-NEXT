//! `pipeline` — 流水线参数 + 工作区运行控制 + 发送任务注册 + 触发器匹配 Tauri 命令
//!
//! 层级: L4 cmd — Tauri IPC 薄适配; 允许依赖 L0-L3, 禁止被任何非 cmd crate 依赖。

mod pipeline;
mod run;
mod trigger;

pub use pipeline::*;
pub use run::*;
pub use trigger::*;
