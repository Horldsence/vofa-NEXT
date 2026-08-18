//! ISO-TP (ISO 15765-2) 传输层模块
//!
//! - `isotp_core`: ISO-TP 核心状态机、常量与公开 API

pub mod isotp_core;

pub use isotp_core::{IsoTpSession, IsoTpSessionHandle};
