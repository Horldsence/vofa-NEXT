//! WGSL 生成测试 — 代码结构断言 + 资格分析边界

use std::collections::BTreeMap;

use node_kind::MathOp;
use node_lower::CompiledOp;

use crate::elig::{plan_unit, testutil, GpuUnitPlan};
use crate::wgsl::emit_module;

/// ps 槽位 (0..3) 归 prelude, 其余槽位归单元 1
fn owners() -> Vec<u32> {
    let mut v = vec![1u32; 32];
    v[0] = 0;
    v[1] = 0;
    v[2] = 0;
    v[3] = 0;
    v
}

fn plan(gi: usize, ops: &[CompiledOp]) -> Option<GpuUnitPlan> {
    let unit = testutil::unit_from_ops(ops);
    plan_unit(gi, ops, &unit, 1, &owners())
}

#[test]
fn 纯math单元生成直线kernel() {
    let ops = [
        testutil::math(MathOp::Add, &[Some(0), Some(1), Some(2)], 4),
        testutil::math(MathOp::Mul, &[Some(4), Some(0)], 5),
        testutil::math(MathOp::Sin, &[Some(5)], 6),
    ];
    let p = plan(0, &ops).expect("纯 Math 单元应有资格");
    assert_eq!(p.in_slots, vec![0, 1, 2], "in 槽位 = prelude 并集");
    assert_eq!(p.out_slots, vec![4, 5, 6]);
    assert_eq!(p.slot_span, 7);
    assert_eq!(p.max_arity, 3);

    let pos: BTreeMap<u32, u32> = [(0u32, 0u32), (1, 1), (2, 2)].into_iter().collect();
    let out_pos: BTreeMap<u32, u32> = [(4u32, 0u32), (5, 1), (6, 2)].into_iter().collect();
    let wgsl = emit_module(&p, &pos, &out_pos);
    assert!(wgsl.contains("@compute @workgroup_size(64u)"), "{wgsl}");
    assert!(wgsl.contains("slots[4u] = acc;"), "{wgsl}");
    assert!(wgsl.contains("slots[6u] = sin(trig_arg(v0));"), "{wgsl}");
    // 三角范围约减存在 (Metal 快速三角大参数无精度保证)
    assert!(wgsl.contains("fn trig_arg"), "{wgsl}");
    // 图级行号写入 (多单元共享下载矩阵)
    assert!(wgsl.contains("out_mat[OUT_ROWS[o] * params.n + t]"), "{wgsl}");
    // 上传行读取: IN_SLOTS/IN_COLS 常量数组
    assert!(
        wgsl.contains("const IN_SLOTS = array<u32, 3u>(0u, 1u, 2u);"),
        "{wgsl}"
    );
    assert!(wgsl.contains("slots[IN_SLOTS[s]] = in_mat[IN_COLS[s] * params.n + t];"));
    assert!(wgsl.contains("out_mat[OUT_ROWS[o] * params.n + t] = slots[OUT_SLOTS[o]];"));
}

#[test]
fn 除零与min_max守卫存在() {
    let ops = [
        testutil::math(MathOp::Div, &[Some(0), Some(1)], 4),
        testutil::math(MathOp::Min, &[Some(0), Some(1)], 5),
        testutil::math(MathOp::Max, &[Some(0), Some(1)], 6),
        testutil::math(MathOp::Log, &[Some(1)], 7),
        testutil::math(MathOp::Sqrt, &[Some(1)], 8),
    ];
    let p = plan(0, &ops).expect("应有资格");
    let pos: BTreeMap<u32, u32> = [(0u32, 0u32), (1, 1)].into_iter().collect();
    let out_pos: BTreeMap<u32, u32> = [(4u32, 0u32), (5, 1), (6, 2), (7, 3), (8, 4)].into_iter().collect();
    let wgsl = emit_module(&p, &pos, &out_pos);
    // Div 零除守卫 / Min-Max ±inf bitcast / Log-Sqrt 定义域守卫
    assert!(
        wgsl.contains("acc = select(acc / vals[i], 0.0, vals[i] == 0.0);"),
        "{wgsl}"
    );
    assert!(wgsl.contains("bitcast<f32>(0x7f800000u)"), "{wgsl}");
    assert!(wgsl.contains("bitcast<f32>(0xff800000u)"), "{wgsl}");
    assert!(wgsl.contains("select(log(v0), 0.0, v0 <= 0.0)"), "{wgsl}");
    assert!(wgsl.contains("select(sqrt(v0), 0.0, v0 < 0.0)"), "{wgsl}");
    // 空集守卫 (Min/Max 全 NaN → 0.0, 与 CPU 提前返回一致)
    assert!(wgsl.matches("if (cnt == 0u) {").count() >= 3, "{wgsl}");
}

#[test]
fn 有状态算子_单元0_跨单元读_均无资格() {
    // Filter 混入 → None
    let filter_ops = vec![
        testutil::math(MathOp::Add, &[Some(0)], 4),
        CompiledOp::Ifft {
            node_id: "i1".into(),
            out: 5,
        },
    ];
    assert!(plan(0, &filter_ops).is_none());
    // 单元 0 (prelude) 恒无资格
    let unit = testutil::unit_from_ops(&[testutil::math(MathOp::Add, &[Some(0)], 4)]);
    assert!(plan_unit(
        0,
        &[testutil::math(MathOp::Add, &[Some(0)], 4)],
        &unit,
        0,
        &owners()
    )
    .is_none());
    // 读另一计算单元的槽位 (owner 2) → None
    let mut own = owners();
    own[9] = 2;
    let unit = testutil::unit_from_ops(&[testutil::math(MathOp::Add, &[Some(9)], 4)]);
    assert!(plan_unit(
        0,
        &[testutil::math(MathOp::Add, &[Some(9)], 4)],
        &unit,
        1,
        &own
    )
    .is_none());
    // 读本单元后续才写的槽位 (拓扑序违反) → None
    let backward = [testutil::math(MathOp::Add, &[Some(5)], 4)];
    assert!(plan(0, &backward).is_none());
    // 未连接输入 (None = 常量 0.0) → 有资格
    let consts = [testutil::math(MathOp::Mul, &[None, None], 4)];
    assert!(plan(0, &consts).is_some());
}
