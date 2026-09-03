//! # engine
//!
//! VOFA-NEXT 节点图引擎 — 三段式编译流水线门面: HIR → 平面 MIR → 后端产物。
//!
//! 各段已拆分为独立 crate, 本 crate 驱动流水线并保有编译产物与慢路径求值:
//! - [`hir`]: 前端 — TypedGraph (petgraph StableDiGraph): id interning +
//!   双角色节点 (字节/数值平面定义同槽共存) + 端口域解析 + 边分类
//! - [`plane`]: 中端 — 值平面/字节平面的结构性投影 (拓扑排序 + 完整环
//!   路径诊断; 跨平面不构成循环由投影保证) + 字节平面 BytePlan
//! - [`lower`]: 后端 — SlotArena 槽位分配 (f32/字符串双 arena) +
//!   per-kind lowering → 平坦 SlotPlan / CompiledOp 序列
//! - [`eval`]: 运行时 — CompiledEval 逐帧槽位评估 (f32 热路径) +
//!   ValuesMap 快照物化
//!
//! 本 crate:
//! - [`compile`]: CompiledGraph 编译 facade (流水线驱动 + 节点查询访问器)
//! - [`evaluate`]: 慢路径图求值 + NodeArm 分发表 — CompiledGraph::evaluate /
//!   evaluate_into 语义参考实现
//! - [`traits`] / [`prelude`]: 共用范式 trait 形状与统一导入面
//!
//! 跨模块测试共享:
//! - `testkit`: 节点/边/帧源构造器 (dev-dependency)
//! - `compile_tests` / `equiv_tests` / eval 测试集: 全流水线测试

mod compile;
mod prelude;
mod traits;

pub mod evaluate;

// ============ 公开 re-export (保持既有调用面不变) ============

pub use compile::CompiledGraph;
pub use eval::{
    node_out_entry, node_out_str_entry, set_port, set_str_port, CompiledEval, SourceFramesMap,
    SourceTextsMap, StringValuesMap, ValuesMap,
};
pub use hir::{
    port_domain_event, CompileError, CompileReport, EdgeClass, HirEdge, HirNode, TypedGraph,
};
pub use lower::CompiledOp;
pub use plane::{BytePlan, ByteRoute};

/// 测试模块经 `use super::*` 链共享的根级导入 (保持拆分前行为)
#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
mod compile_tests;
#[cfg(test)]
mod equiv_tests;
#[cfg(test)]
mod tests {
    use super::*;

    mod eval_custom_tests;
    mod eval_filter_tests;
    mod eval_input_tests;
    mod eval_math_tests;
    mod eval_misc_tests;
    mod eval_str_tests;
    mod eval_trigger_tests;
}
