//! # hir
//!
//! 节点图编译前端 — HIR 类型化图。
//!
//! 以 petgraph StableDiGraph 承载前端同步来的节点图:
//! - [`hir`]: id interning + 双角色节点 (字节/数值平面定义同槽共存) +
//!   端口域解析 + 边分类 → [`EdgeClass`]
//! - [`errors`]: 编译错误薄壳 — `CompileError` 定义在 `error` crate,
//!   本 crate 保留转换边界 (`kind::PortDomain` → 事件契约 DTO)
//!
//! 容错语义: 边端点节点缺失时创建占位节点 (无双角色定义), 端口域按 F32 处理。

mod errors;
mod hir;

pub use errors::{port_domain_event, CompileError, CompileReport};
pub use hir::{EdgeClass, Hir, HirEdge, HirNode, TypedGraph};
