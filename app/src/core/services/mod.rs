//! # services — 应用服务层（按领域拆分）
//!
//! 从 Tauri commands 迁移而来: 函数直接接收 `&AppState`, 供 egui UI 调用。
//! 模块根声明子模块并通过 `pub use` 将所有服务函数重导出到 `services::` 命名空间。

mod buffer;
mod can;
mod can_load;
mod frame_decoder;
mod graph;
mod logic;
mod protocol;
mod transport;

pub use buffer::*;
pub use can::*;
pub use can_load::*;
pub use frame_decoder::*;
pub use graph::*;
pub use logic::*;
pub use protocol::*;
pub use transport::*;
