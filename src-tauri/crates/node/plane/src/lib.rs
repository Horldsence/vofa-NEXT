//! # plane
//!
//! 节点图编译中端 — 平面投影 (MIR)。
//!
//! 以 [`hir::EdgeClass`] 的边分类为谓词, 从 HIR 结构性投影出互不成环的
//! 两个平面子图 (各平面只含本平面边), petgraph 拓扑排序后输出:
//! - [`plane`]: 值平面 (f32 ∪ 字符串 ∪ RawData 数值标记) → [`ValueMir`]
//!   (拓扑序 + 输入反查索引 + 编译期端口名缓存)
//! - [`byte_plan`]: 字节平面 (Bytes 边) → [`BytePlan`] / [`ByteRoute`]
//!   (拓扑序 + 源→下游路由表 + O(1) 成员查询)
//!
//! 环诊断: [`CompileError::Cycle`] / [`CompileError::ByteCycle`]
//! 携带完整环路径 (`a → b → a`), 由三色 DFS 提取。

mod byte_plan;
mod plane;

pub use byte_plan::{BytePlan, ByteRoute};
pub use plane::{byte_plane_order, value_plane, ValueMir};
