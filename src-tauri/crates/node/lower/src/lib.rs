//! # lower
//!
//! 节点图编译后端 — 值平面 MIR → 槽位产物。
//!
//! 结构: [`lower::SlotArena`] 槽位分配器 (f32/字符串双 arena) + 按节点种类分派的
//! per-kind lowering ([`kinds`] 子模块, 见 `kinds::lower_node`)。
//! 输入边在编译期经 [`plane::ValueMir`] 反查索引解析为槽位下标;
//! 查不到 = 常量 0.0 (与慢路径 resolve_input 缺省语义一致, 以 None 表示)。
//!
//! 产物: [`SlotPlan`] — 平坦 [`CompiledOp`] 序列 + 双域槽位表 + 帧源表,
//! 由 `eval` 封装为逐帧评估的 CompiledEval。

mod lower;
mod ops;
mod units;

pub mod kinds;

pub use lower::{lower_value_plane, LowerCtx, SlotArena, SlotPlan};
pub use ops::{op_weight, CompiledOp, EvalUnit, TextOutSpec};
