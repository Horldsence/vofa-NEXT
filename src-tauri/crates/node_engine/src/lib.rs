//! # node_engine
//!
//! VOFA-NEXT 节点图引擎 — 字节平面编译 + 数值平面求值。
//!
//! 核心模块:
//! - [`byte_plan`]: 字节平面路由 (BytePlan / ByteRoute) — 拓扑序 + 跨平面互不循环
//! - [`compile`]: CompiledGraph 编译 (字节边/f32 边分类 + 拓扑排序 + 槽位表构建)
//! - [`eval`]: 编译期槽位评估表 (CompiledEval) — f32 热路径
//! - [`evaluate`]: 慢路径图求值 + 节点查询 — CompiledGraph::evaluate / evaluate_into / 配置访问器
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
mod eval;
mod evaluate;

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
pub use compile::{CompileError, CompiledGraph};
pub use eval::{CompiledEval, CompiledOp, SourceFramesMap, SourceTextsMap};
