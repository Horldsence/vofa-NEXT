//! # node_engine
//!
//! VOFA-NEXT 节点图引擎 — 三段式编译流水线: HIR → 平面 MIR → 后端产物。
//!
//! 编译流水线:
//! - [`hir`]: 前端 — TypedGraph (petgraph StableDiGraph): id interning +
//!   双角色节点 (字节/数值平面定义同槽共存) + 端口域解析 + 边分类
//! - [`plane`]: 中端 — 值平面/字节平面的 EdgeFiltered 零拷贝投影 + petgraph
//!   拓扑排序 + 完整环路径诊断; 跨平面不构成循环由投影结构性保证
//! - [`lower`] / [`lower::kinds`]: 后端 — SlotArena 槽位分配 (f32/字符串双 arena)
//!   + per-kind lowering → 平坦 [`CompiledOp`] 序列
//!
//! 产物与运行时:
//! - [`compile`]: CompiledGraph 编译 facade (流水线驱动 + 节点查询访问器)
//! - [`byte_plan`]: 字节平面路由 (BytePlan / ByteRoute) — 拓扑序 + O(1) 成员查询
//! - [`eval`]: 编译期槽位评估表 (CompiledEval) — f32 热路径
//! - [`evaluate`]: 慢路径图求值 + NodeArm 分发表 — CompiledGraph::evaluate / evaluate_into
//! - [`errors`]: CompileError — 强类型变体, 完整环路径诊断
//! - [`ValuesMap`][]: 输出值表类型别名 (FxHash 优化)
//! - [`StringValuesMap`][]: 字符串输出值表类型别名 (Str 节点 String 域输出)
//! - [`SourceFramesMap`][] / [`SourceTextsMap`][]: 每源最新帧/文本缓存类型别名
//!   (ProtocolSource 的数值/字符串端口数据源)
//!
//! 跨模块测试共享:
//! - [`test_helpers`]: pub(crate) 节点/边/帧源构造器
//! - [`compile_tests`] / [`eval_tests`] / [`equiv_tests`]: 各模块测试集

mod byte_plan;
mod compile;
mod errors;
mod eval;
pub mod evaluate;
mod hir;
pub mod lower;
mod ops;
mod plane;
mod prelude;
mod traits;

#[cfg(test)]
mod compile_tests;
#[cfg(test)]
mod equiv_tests;
#[cfg(test)]
mod eval_tests;

#[cfg(test)]
pub(crate) mod test_helpers;

use rustc_hash::FxBuildHasher;
use std::collections::HashMap;

/// 图输出值表 (热路径) — FxHash 替代 SipHash, 高码率逐帧覆盖写时查找快 3~5 倍。
/// serde 对任意 BuildHasher+Default 的 HashMap 透明, 线上 JSON 格式不变。
pub type ValuesMap = HashMap<String, HashMap<String, f32, FxBuildHasher>, FxBuildHasher>;

/// 字符串输出值表 — Str 节点 String 域输出 (widgetId → portId → text), 仿 [`ValuesMap`]
pub type StringValuesMap = HashMap<String, HashMap<String, String, FxBuildHasher>, FxBuildHasher>;

// ============ 公开 re-export ============

pub use byte_plan::{BytePlan, ByteRoute};
pub use compile::CompiledGraph;
pub use errors::CompileError;
pub use eval::{CompiledEval, SourceFramesMap, SourceTextsMap};
pub use hir::TypedGraph;
pub use ops::CompiledOp;
