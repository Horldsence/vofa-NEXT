//! 图编译错误薄壳 — `CompileError` / `CompileReport` / `PortDomain` 的强类型定义在
//! `error` crate (`pub use compile::...`), 本模块仅保留历史调用面 + `node_kind::PortDomain`
//! → `error::PortDomain` 的边界转换 (事件契约需要 serde-friendly DTO, 但 `error` crate 不依赖
//! `node_kind` 以避免循环).
//!
//! 调用方:
//! - `node_engine` 内部 (hir / compile / plane / byte_plan): `use crate::errors::CompileError`
//! - `cmd_graph::update_tab_graph`: 把错误包为 `ConfigError::GraphCompile(Box<CompileError>)`
//! - 前端 IPC: 经 `error::AppError::Graph(Boxed)` 序列化

pub use error::{CompileError, CompileReport, PortDomain};

/// `node_kind::PortDomain` → 事件契约 `error::PortDomain` (serde-friendly DTO).
/// 在 `CompileError::DomainMismatch` 构造处调用.
pub fn port_domain_event(d: node_kind::PortDomain) -> PortDomain {
    match d {
        node_kind::PortDomain::F32 => PortDomain::F32,
        node_kind::PortDomain::Bytes => PortDomain::Bytes,
        node_kind::PortDomain::String => PortDomain::String,
    }
}
