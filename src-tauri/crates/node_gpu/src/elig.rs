//! GPU 资格分析 — 判定评估单元能否整段编译为 WGSL kernel
//!
//! v1 标准: 单元 op 区段全部为 [`node_lower::CompiledOp::Math`], 且每个输入
//! 槽位要么由前导单元 (prelude, `slot_unit == 0`) 供给, 要么由本单元在拓扑
//! 序先前的 op 写出 (编译期切分不变量保证跨单元读只指向 prelude 槽位)。
//! 有状态算子 (Filter/Ifft/Trigger/FrameDecoder/Str) 不在范围 — GPU 上只能
//! 按节点串行, 收益存疑且语义风险高。

use node_kind::MathOp;
use node_lower::{CompiledOp, EvalUnit};

/// Math op 的 GPU 平面表示 — (`MathOp`, 输入槽位表, 输出槽位)
///
/// 输入 `None` = 未连接 (常量 0.0, 与 CPU 语义一致)
pub struct GpuMathOp {
    /// 算子种类
    pub op: MathOp,
    /// 输入槽位 (None = 常量 0.0)
    pub inputs: Vec<Option<u32>>,
    /// 输出槽位
    pub out: u32,
}

/// 单元的 GPU 执行计划 — 资格分析产物 (只读, Arc 跨线程共享)
pub struct GpuUnitPlan {
    /// 所属图下标 (触发图列表 `graph_list` 中的位置)
    pub gi: usize,
    /// 所属评估单元下标 (`CompiledEval::units()` 中的位置, 恒 > 0)
    pub unit_index: usize,
    /// kernel 从上传矩阵读取的槽位 (升序去重 = 本单元引用的 prelude 供给槽位)
    pub in_slots: Vec<u32>,
    /// kernel 写出的槽位 (升序 = 单元全部写槽位; Math 每帧恒写)
    pub out_slots: Vec<u32>,
    /// kernel 局部槽位地址空间 (被引用槽位最大下标 + 1)
    pub slot_span: u32,
    /// 最大算子输入路数 (WGSL 收集数组尺寸)
    pub max_arity: u32,
    /// op 区段 (拓扑序, WGSL 直线代码按此生成)
    pub math_ops: Vec<GpuMathOp>,
}

/// 资格分析 — 单元不可整体编译时返回 `None` (调用方回退 CPU 单元)
///
/// * `ops`        - `CompiledEval::ops()` 全表
/// * `unit`       - 待分析单元 (op 区段; 下标 0 的 prelude 恒不资格)
/// * `unit_index` - 单元在 `units()` 中的下标
/// * `slot_unit`  - `CompiledEval::slot_unit()` 正本槽位归属表
#[must_use]
pub fn plan_unit(
    gi: usize,
    ops: &[CompiledOp],
    unit: &EvalUnit,
    unit_index: usize,
    slot_unit: &[u32],
) -> Option<GpuUnitPlan> {
    if unit_index == 0 || unit.op_len == 0 {
        return None;
    }
    let start = unit.op_start as usize;
    let segment = ops.get(start..start + unit.op_len as usize)?;

    let mut math_ops = Vec::with_capacity(segment.len());
    let mut in_set = std::collections::BTreeSet::new();
    let mut out_set = std::collections::BTreeSet::new();
    // 本单元已写槽位 (拓扑序内先写后读)
    let mut written: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut max_arity = 1u32;

    for op in segment {
        let CompiledOp::Math {
            op: kind,
            inputs,
            out,
        } = op
        else {
            return None; // 有状态/字符串平面 op → 整单元回退 CPU
        };
        let out = conv_slot(*out)?;
        let mut ins = Vec::with_capacity(inputs.len());
        for s in inputs {
            let slot = match s {
                None => None,
                Some(raw) => {
                    let slot = conv_slot(*raw)?;
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
        max_arity = max_arity.max(u32::try_from(ins.len()).unwrap_or(1));
        written.insert(out);
        out_set.insert(out);
        math_ops.push(GpuMathOp {
            op: *kind,
            inputs: ins,
            out,
        });
    }

    let slot_span = in_set
        .iter()
        .chain(out_set.iter())
        .copied()
        .max()
        .map_or(1, |m| m + 1);
    Some(GpuUnitPlan {
        gi,
        unit_index,
        in_slots: in_set.into_iter().collect(),
        out_slots: out_set.into_iter().collect(),
        slot_span,
        max_arity,
        math_ops,
    })
}

/// 槽位下标 usize → u32 (超界视为不可编译)
fn conv_slot(raw: usize) -> Option<u32> {
    u32::try_from(raw).ok()
}

#[cfg(test)]
pub mod testutil {
    //! 资格分析测试构造 — codegen / 等价测试共用

    use node_kind::MathOp;
    use node_lower::{CompiledOp, EvalUnit};

    /// 手工构造最小单元 (op 区段连续从 0 开始; 写槽位 = 全部 Math out)
    pub fn unit_from_ops(ops: &[CompiledOp]) -> EvalUnit {
        let clear = ops
            .iter()
            .filter_map(|op| match op {
                CompiledOp::Math { out, .. } => Some(u32::try_from(*out).unwrap_or(u32::MAX)),
                _ => None,
            })
            .collect();
        EvalUnit {
            op_start: 0,
            op_len: u32::try_from(ops.len()).unwrap_or(1),
            clear_slots: clear,
            clear_str_slots: Vec::new(),
            filter_ids: Vec::new(),
            ifft_ids: Vec::new(),
            trigger_ids: Vec::new(),
            weight: 1,
        }
    }

    /// Math op 便捷构造
    pub fn math(op: MathOp, inputs: &[Option<usize>], out: usize) -> CompiledOp {
        CompiledOp::Math {
            op,
            inputs: inputs.to_vec(),
            out,
        }
    }
}
