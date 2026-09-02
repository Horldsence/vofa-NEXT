//! SIMD 资格分析 — 判定评估单元能否整段批量 (SoA) 求值
//!
//! 标准与原 wgpu 资格分析 (git 历史 `node_gpu::elig::plan_unit`) 一致:
//! 单元 op 区段全部为 [`node_lower::CompiledOp::Math`], 且每个输入槽位要么
//! 由前导单元 (prelude, `slot_unit == 0`) 供给, 要么由本单元在拓扑序先前的
//! op 写出 (编译期切分不变量保证跨单元读只指向 prelude 槽位)。
//! 有状态算子 (Filter/Ifft/Trigger/FrameDecoder/Str) 不在范围 — 语义上必须
//! 逐帧串行。

use std::collections::BTreeSet;

use node_kind::MathOp;
use node_lower::{CompiledOp, EvalUnit};

/// Math op 的 SIMD 平面表示 — (`MathOp`, 输入槽位表, 输出槽位)
///
/// 输入 `None` = 未连接 (常量 0.0, 非 NaN, 与 CPU 语义一致)
pub struct SimdMathOp {
    /// 算子种类
    pub op: MathOp,
    /// 输入槽位 (None = 常量 0.0)
    pub inputs: Vec<Option<usize>>,
    /// 输出槽位
    pub out: usize,
}

/// 单元的 SIMD 执行计划 — 资格分析产物
pub struct SimdUnitPlan {
    /// 评估单元下标 (`CompiledEval::units()` 中的位置, 恒 > 0)
    pub unit_index: usize,
    /// op 区段 (拓扑序, 按此顺序批量求值)
    pub ops: Vec<SimdMathOp>,
    /// prelude 供给的输入槽位 (升序去重; gather 阶段从槽位副本逐帧取数)
    pub in_slots: Vec<usize>,
    /// 单元全部写槽位 (升序; Math 每帧恒写; scatter 阶段逐帧回写槽位副本)
    pub out_slots: Vec<usize>,
}

/// 资格分析 — 单元不可批量求值时返回 `None` (调用方整单元回退逐帧标量)
///
/// * `ops`        - `CompiledEval::ops()` 全表
/// * `unit`       - 待分析单元 (op 区段; 下标 0 的 prelude 恒不资格)
/// * `unit_index` - 单元在 `units()` 中的下标
/// * `slot_unit`  - `CompiledEval::slot_unit()` 正本槽位归属表
#[must_use]
pub fn plan_unit(
    ops: &[CompiledOp],
    unit: &EvalUnit,
    unit_index: usize,
    slot_unit: &[u32],
) -> Option<SimdUnitPlan> {
    if unit_index == 0 || unit.op_len == 0 {
        return None;
    }
    let start = unit.op_start as usize;
    let segment = ops.get(start..start + unit.op_len as usize)?;

    let mut math_ops = Vec::with_capacity(segment.len());
    let mut in_set = BTreeSet::new();
    let mut out_set = BTreeSet::new();
    // 本单元已写槽位 (拓扑序内先写后读)
    let mut written: BTreeSet<usize> = BTreeSet::new();

    for op in segment {
        let CompiledOp::Math {
            op: kind,
            inputs,
            out,
        } = op
        else {
            return None; // 有状态/字符串平面 op → 整单元回退逐帧标量
        };
        let mut ins = Vec::with_capacity(inputs.len());
        for s in inputs {
            let slot = match s {
                None => None,
                Some(raw) => {
                    let slot = *raw;
                    // 跨单元读只允许 prelude 供给 (编译期不变量的静态复核)
                    let is_prelude = slot_unit.get(*raw).is_some_and(|&u| u == 0);
                    if !is_prelude && !written.contains(&slot) {
                        return None;
                    }
                    if is_prelude {
                        in_set.insert(slot);
                    }
                    Some(slot)
                }
            };
            ins.push(slot);
        }
        written.insert(*out);
        out_set.insert(*out);
        math_ops.push(SimdMathOp {
            op: *kind,
            inputs: ins,
            out: *out,
        });
    }

    Some(SimdUnitPlan {
        unit_index,
        ops: math_ops,
        in_slots: in_set.into_iter().collect(),
        out_slots: out_set.into_iter().collect(),
    })
}
