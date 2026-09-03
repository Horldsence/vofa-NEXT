//! # diagnostic
//!
//! 诊断协议数据类型 — ISO-TP / UDS / OBD-II / J1939 统一事件模型。
//!
//! 这些类型由 `automotive` crate 产生,通过 Tauri Channel 推送到前端。
//! 字段命名与前端 `src/types/index.ts` 中的 `DiagnosticMessage` 联合类型对齐
//! (snake_case)。
//!
//! 模块:
//! - [`uds`]: UDS 服务 ID 与否定响应码
//! - [`obd`]: OBD-II 模式 + DTC
//! - [`j1939`]: J1939 PGN / SPN
//! - [`message`]: `DiagnosticMessage` 联合类型 + 批次
//! - [`config`]: ISO-TP / UDS / OBD-II / J1939 配置

pub mod config;
pub mod j1939;
pub mod message;
pub mod obd;
pub mod uds;

pub use config::{
    DiagnosticConfig, IsoTpAddressMode, IsoTpConfig, J1939Config, ObdConfig, UdsConfig,
};
pub use j1939::{J1939Id, J1939Spn};
pub use message::{DiagnosticMessage, DiagnosticMessageBatch};
pub use obd::{Dtc, DtcStatus, ObdMode};
pub use uds::{UdsNrc, UdsService};
