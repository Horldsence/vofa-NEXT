//! CPU/GPU 数值等价测试 — 全部 15 算子 × NaN/零值/负值边界
//!
//! 契约: 纯 ALU 算子 (Add/Sub/Mul/Min/Max/Abs/Neg/Square) 与 CPU 位级一致;
//! Div/Avg (Metal 除法非精确舍入, WGSL 规范允许 2.5 ULP) 与超越函数
//! (Sqrt/Sin/Cos/Tan/Log) 允许对称相对误差 < 1e-5。无适配器环境 (CI) 跳过。

use std::collections::BTreeMap;

use node_kind::MathOp;
use node_lower::CompiledOp;

use crate::elig::{plan_unit, testutil};
use crate::runner::GpuSession;

/// 覆盖全部 15 算子的链 — 输出槽位 10..25
fn chain_ops() -> Vec<CompiledOp> {
    vec![
        testutil::math(MathOp::Add, &[Some(0), Some(1), Some(2)], 10),
        testutil::math(MathOp::Sub, &[Some(10), Some(3)], 11),
        testutil::math(MathOp::Mul, &[Some(11), Some(3), Some(0)], 12),
        testutil::math(MathOp::Div, &[Some(12), Some(1)], 13),
        testutil::math(MathOp::Avg, &[Some(13), Some(10), Some(11)], 14),
        testutil::math(MathOp::Min, &[Some(14), Some(0), Some(1), Some(2)], 15),
        testutil::math(MathOp::Max, &[Some(15), Some(0), Some(1), Some(2)], 16),
        testutil::math(MathOp::Abs, &[Some(16)], 17),
        testutil::math(MathOp::Neg, &[Some(17)], 18),
        testutil::math(MathOp::Square, &[Some(18)], 19),
        testutil::math(MathOp::Sqrt, &[Some(19)], 20),
        testutil::math(MathOp::Sin, &[Some(2)], 21),
        testutil::math(MathOp::Cos, &[Some(21)], 22),
        testutil::math(MathOp::Tan, &[Some(22)], 23),
        testutil::math(MathOp::Log, &[Some(3)], 24),
    ]
}

/// 确定性伪随机输入 — 覆盖 NaN / 零 / 负值边界 (帧 0 与帧 5 为特例)
#[allow(clippy::cast_precision_loss)]
fn prelude_value(slot: usize, frame: usize) -> f32 {
    // 线性同余 — 可复现 (u32 域)
    let f = u32::try_from(frame).unwrap_or(0);
    let s = u32::try_from(slot).unwrap_or(0);
    let x = (f.wrapping_add(s.wrapping_mul(4_072_555_247)) ^ 0x05f3_7594) as f32 / u32::MAX as f32;
    match (slot, frame) {
        // 全 NaN 帧 → MathOp 空集守卫 (Add/Min/Max 等应返回 0.0)
        (0..=3, 0) => f32::NAN,
        // 除零帧 (slot 1 为 Div 分母) → 守卫 0.0
        (1, 1) => 0.0,
        // Log 定义域边界 (≤ 0 → 0.0)
        (3, 2) => -1.5,
        (3, 3) => 0.0,
        // 部分NaN (过滤后仍有可用输入)
        (2, 4) => f32::NAN,
        // 超越函数安全域 (tan 远离渐近线, log > 0)
        (2, _) => 1.2f32.mul_add(x, 0.05),
        (3, _) => 3.0f32.mul_add(x, 0.05),
        _ => 4.0f32.mul_add(x, -2.0),
    }
}

/// CPU 参照 — 复刻 prelude 供给 + `CompiledEval::run_ops` 的 Math 臂语义
fn cpu_eval(in_slots: &[u32], plan_slots: &[u32], n: usize) -> Vec<Vec<f32>> {
    let ops = chain_ops();
    let mut slots = [0.0f32; 32];
    let mut cols: Vec<Vec<f32>> = Vec::with_capacity(n);
    for frame in 0..n {
        slots.fill(0.0);
        // prelude 供给槽位 (与上传矩阵同源)
        for &s in in_slots {
            slots[s as usize] = prelude_value(s as usize, frame);
        }
        for op in &ops {
            let CompiledOp::Math {
                op: kind,
                inputs,
                out,
            } = op
            else {
                continue;
            };
            let mut buf = [0.0f32; 8];
            for (i, s) in inputs.iter().enumerate() {
                buf[i] = s.map_or(0.0, |s| slots[s]);
            }
            slots[*out] = kind.evaluate(&buf[..inputs.len()]);
        }
        cols.push(
            plan_slots
                .iter()
                .map(|slot| slots[*slot as usize])
                .collect(),
        );
    }
    // 转置为 [row][frame]: outs[r][f] = cols[f][r]
    let mut outs = vec![vec![0.0; n]; plan_slots.len()];
    for (row, out_row) in outs.iter_mut().enumerate() {
        for (cell, col) in out_row.iter_mut().zip(&cols) {
            *cell = col[row];
        }
    }
    outs
}

/// 对称相对误差 (超越函数容差; bit-eq / 双 NaN 直接通过)
#[allow(clippy::float_cmp)] // 位级一致正是本测试契约
fn close_enough(a: f32, b: f32) -> bool {
    if a == b || (a.is_nan() && b.is_nan()) {
        return true;
    }
    if a.is_nan() || b.is_nan() || a.is_infinite() || b.is_infinite() {
        return false;
    }
    let rel = (a - b).abs() / (a.abs() + b.abs());
    rel < 1e-5
}

#[test]
fn gpu全算子与cpu等价() {
    let Some(ctx) = gpu_core::GpuContext::acquire() else {
        eprintln!("无 GPU 适配器, 跳过等价测试");
        return;
    };
    let ops = chain_ops();
    // 槽位归属: 0..3 = prelude 供给, 其余 (含中间槽位 10..24) = 本单元
    let mut owners = vec![1u32; 32];
    owners[0] = 0;
    owners[1] = 0;
    owners[2] = 0;
    owners[3] = 0;
    let unit = testutil::unit_from_ops(&ops);
    let plan = plan_unit(0, &ops, &unit, 1, &owners).expect("链应有资格");
    let in_slots = plan.in_slots.clone();
    let out_slots = plan.out_slots.clone();
    assert_eq!(in_slots, vec![0, 1, 2, 3]);
    assert_eq!(out_slots.len(), 15);

    let n = 257usize; // 非整 workgroup (257 = 4×64 + 1) 覆盖尾部掩码分支
    let mut in_mat = vec![0.0f32; in_slots.len() * n];
    for frame in 0..n {
        for (col, slot) in in_slots.iter().enumerate() {
            in_mat[col * n + frame] = prelude_value(*slot as usize, frame);
        }
    }

    let mut session = GpuSession::build(
        ctx,
        &[(String::from("t"), vec![std::sync::Arc::new(plan)])],
    );
    session
        .enqueue("t", u32::try_from(n).unwrap_or(1), &in_mat)
        .expect("enqueue");
    session.finish_chunk().expect("finish");
    let mut out_mat = vec![0.0f32; out_slots.len() * n];
    session.read_out("t", &mut out_mat);

    // CPU 参照 + 逐算子比对
    let expected = cpu_eval(&in_slots, &out_slots, n);
    // 位级一致: 纯 ALU (无除法) — Metal/WGSL 对 +−× 与 min/max/abs 保证精确舍入
    let exact: [MathOp; 8] = [
        MathOp::Add,
        MathOp::Sub,
        MathOp::Mul,
        MathOp::Min,
        MathOp::Max,
        MathOp::Abs,
        MathOp::Neg,
        MathOp::Square,
    ];
    let pos: BTreeMap<u32, u32> = out_slots
        .iter()
        .enumerate()
        .map(|(i, s)| (*s, u32::try_from(i).unwrap_or(0)))
        .collect();
    let _ = pos;
    for (row, slot) in out_slots.iter().enumerate() {
        // 找产出该槽位的算子 (链定义: out = 10 + op 序号)
        let op_idx = usize::try_from(slot - 10).unwrap_or(0);
        let CompiledOp::Math { op: kind, .. } = &ops[op_idx] else {
            continue;
        };
        let tolerance = !exact.contains(kind);
        let gpu_row = &out_mat[row * n..][..n];
        for (frame, (gpu, cpu)) in gpu_row.iter().zip(&expected[row]).enumerate() {
            if tolerance {
                assert!(
                    close_enough(*gpu, *cpu),
                    "slot {slot} ({kind:?}) frame {frame}: gpu {gpu} vs cpu {cpu}"
                );
            } else {
                assert_eq!(
                    gpu.to_bits(),
                    cpu.to_bits(),
                    "slot {slot} ({kind:?}) frame {frame}: gpu {gpu} vs cpu {cpu} 应位级一致"
                );
            }
        }
    }
}

/// 大参数三角范围约减回归 — Metal 快速 sin 对 |x| > 100 无精度保证,
/// WGSL 端 2π 约减后须与 CPU 对齐 (≤1e-5 相对); 无适配器跳过
#[test]
fn sin_large_argument_range_reduction() {
    let Some(ctx) = gpu_core::GpuContext::acquire() else {
        eprintln!("无 GPU 适配器, 跳过范围约减测试");
        return;
    };
    // 单 Sin 算子: in slot 0 → out slot 1
    let ops = vec![testutil::math(MathOp::Sin, &[Some(0)], 1)];
    let mut owners = vec![1u32; 8];
    owners[0] = 0;
    let unit = testutil::unit_from_ops(&ops);
    let Some(plan) = plan_unit(0, &ops, &unit, 1, &owners) else {
        panic!("应有资格");
    };
    let out_len = plan.out_slots.len();
    let n = 4usize;
    let args = [-128.0f32, -2.336_288_5, 127.0, 3.946_891];
    let mut in_mat = vec![0.0f32; n];
    in_mat.copy_from_slice(&args);
    let mut session = GpuSession::build(
        ctx,
        &[(String::from("0"), vec![std::sync::Arc::new(plan)])],
    );
    session
        .enqueue("0", u32::try_from(n).unwrap_or(1), &in_mat)
        .expect("enqueue");
    session.finish_chunk().expect("finish");
    let mut out = vec![0.0f32; out_len * n];
    session.read_out("0", &mut out);
    for (i, a) in args.iter().enumerate() {
        let (g, c) = (out[i], a.sin());
        let rel = (g - c).abs() / (g.abs() + c.abs()).max(1e-9);
        assert!(rel < 1e-5, "sin({a}): gpu {g} vs cpu {c} (rel {rel})");
    }
}
