//! SciRS2 SIMD 批量求值 — fork-join 并行路径内 Math 单元的向量化执行
//!
//! 替代原 wgpu 卸载 (见 git 历史 `node_gpu` / `graph_eval_parallel/gpu.rs`):
//! 相同的资格标准 (全 Math 单元 + 输入仅来自 prelude 或本单元更早写出),
//! 相同的分块 staging 拓扑 (逐帧 prelude + gather → 批量求值 → scatter),
//! 但内核留在 CPU 上 — [`scirs2_core::simd_ops::SimdUnifiedOps`] 自动分发
//! AVX2/NEON, 无设备初始化 / 上传回传 / 会话失败回退。
//!
//! 数值契约: 与标量热路径 ([`node_kind::MathOp::evaluate`]) **逐位一致** —
//! - Add/Mul (任意元): 恒起点链式 SIMD 归约, 精确复刻 `sum()`/`reduce` 的
//!   折叠序 (`-0.0 + a + b` / `1.0 * a * b`, IEEE 加乘逐位一致)
//! - 其余算子/高元: 列式标量循环直接调用 `MathOp::evaluate` (参考函数本身)
//!
//! 因此超越函数无需域约减近似 (对比 WGSL 版的 2π 约减 + 1e-5 容差)。

pub mod ops;
pub mod plans;

use std::collections::BTreeSet;

pub use plans::{plan_unit, SimdUnitPlan};

/// 每图 SIMD 执行计划 — 批内构建 (资格分析 O(ops), 无需跨批缓存)
///
/// 由 [`crate::graph_eval_parallel::plan::build_plans`] 填充并挂到
/// [`super::plan::BucketGraphPlan`] 上; `eval_simd = false` 时为空表。
#[derive(Default)]
pub struct GraphSimdPlan {
    /// 资格单元 (unit_ids 中的子集, 升序)
    pub units: Vec<SimdUnitPlan>,
    /// 资格单元引用的 prelude 供给槽位并集 (升序; gather 阶段用)
    pub in_slots: Vec<usize>,
    /// 资格单元全部写槽位并集 (升序; scatter 阶段用)
    pub out_slots: Vec<usize>,
}

impl GraphSimdPlan {
    /// 对某图全部单元做资格分析, 空计划表示无 SIMD 单元 (整图走标量路径)
    #[must_use]
    pub fn build(compiled: &node_eval::CompiledEval) -> Self {
        let ops = compiled.ops();
        let slot_unit = compiled.slot_unit();
        let units: Vec<SimdUnitPlan> = compiled
            .units()
            .iter()
            .enumerate()
            .filter_map(|(unit_index, unit)| plan_unit(ops, unit, unit_index, slot_unit))
            .collect();
        let mut in_set = BTreeSet::new();
        let mut out_set = BTreeSet::new();
        for unit in &units {
            in_set.extend(unit.in_slots.iter().copied());
            out_set.extend(unit.out_slots.iter().copied());
        }
        Self {
            units,
            in_slots: in_set.into_iter().collect(),
            out_slots: out_set.into_iter().collect(),
        }
    }

    /// 该图是否存在资格单元
    pub const fn is_empty(&self) -> bool {
        self.units.is_empty()
    }
}
