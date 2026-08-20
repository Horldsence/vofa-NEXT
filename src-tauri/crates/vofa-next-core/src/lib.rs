//! # vofa-next-core
//!
//! 历史说明:本 crate 早期集中放置所有共享类型。
//!
//! **拆分进度**(Stage A 第一批已迁出):
//! - [`vofa_core`]        — 基础类型 error/frame/serial_params
//! - [`can_types`]        — CAN 帧/缓冲/负载统计
//! - [`logic_types`]      — 逻辑分析仪类型/缓冲
//! - [`diagnostic`]       — UDS/OBD-II/J1939 配置与消息
//!
//! 新 crate 提供"替代导入路径",可独立使用。本 crate 内部仍持有
//! `pub mod can; pub mod logic;` 等旧模块(含 schema/config)用于过渡,
//! 旧路径 `vofa_next_core::DataFrame` 等仍可工作。
//!
//! **Stage H (清理阶段)** 将删除旧 can/logic/diagnostic 模块,
//! 保留本 crate 作为纯 façade re-export。

pub mod can;
pub mod config;
pub mod diagnostic;
pub mod error;
pub mod frame;
pub mod logic;
pub mod schema;

pub use can::*;
pub use config::*;
pub use diagnostic::*;
pub use error::{Error, Result};
pub use frame::*;
pub use logic::*;
pub use schema::*;
