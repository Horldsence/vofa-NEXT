//! GPU 包络降采样等价性 — wgpu 路径与 CPU 参考实现位级一致
//!
//! 覆盖: 随机波形 (含 NaN) × 多档 (n, columns) 组合、边界 (空输入 / 全 NaN /
//! n < columns / 单列)。无 GPU 适配器环境 (部分 CI) 整组跳过 — CPU 参考的
//! 纯逻辑行为由 envelope.rs 内嵌单测覆盖。

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)] // LCG 伪随机数据流: 有意截断

use gpu_core::{envelope_minmax, envelope_minmax_cpu, GpuContext};

/// 确定性伪随机波形 (正弦 + LCG 噪声, 每 997 点注入 NaN, 每 1273 点注入 ±0)
fn waveform(n: usize) -> Vec<f32> {
    let mut seed = 0xa5a5_5a5a_u32;
    (0..n)
        .map(|i| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            if i % 997 == 0 {
                return f32::NAN;
            }
            if i % 1273 == 0 {
                return if i % 2 == 0 { 0.0 } else { -0.0 };
            }
            let noise = ((seed >> 8) % 20_000) as f32 / 100.0 - 100.0;
            (f32::from((i % 10_000) as u16) * 0.001 * std::f32::consts::TAU).sin() + noise * 0.01
        })
        .collect()
}

/// 位级等价断言 (含 ±inf / ±0 的符号位)
fn assert_envelope_eq(name: &str, gpu: &gpu_core::Envelope, cpu: &gpu_core::Envelope) {
    assert_eq!(gpu.min.len(), cpu.min.len(), "{name}: min 长度");
    assert_eq!(gpu.count, cpu.count, "{name}: count");
    for i in 0..cpu.min.len() {
        assert_eq!(
            gpu.min[i].to_bits(),
            cpu.min[i].to_bits(),
            "{name}: min[{i}] 位级不一致"
        );
        assert_eq!(
            gpu.max[i].to_bits(),
            cpu.max[i].to_bits(),
            "{name}: max[{i}] 位级不一致"
        );
    }
}

#[test]
fn gpu_matches_cpu_bitwise() {
    let Some(ctx) = GpuContext::acquire() else {
        eprintln!("无 GPU 适配器, 跳过 GPU 包络等价测试");
        return;
    };
    // (n, columns) 组合: 典型窗口量级 + 列数/样本数倒挂 + 单列
    for (n, columns) in [
        (1usize, 1usize),
        (17, 64),
        (1000, 64),
        (4096, 512),
        (65_536, 2048),
        (100_003, 2000),
        (0, 8),
    ] {
        let values = waveform(n);
        let gpu = envelope_minmax(&ctx, &values, columns).expect("GPU 包络应成功");
        let cpu = envelope_minmax_cpu(&values, columns);
        assert_envelope_eq(&format!("n={n} columns={columns}"), &gpu, &cpu);
    }

    // 全 NaN: 每列空 (min=+inf / max=-inf / count=0)
    let values = vec![f32::NAN; 4096];
    let gpu = envelope_minmax(&ctx, &values, 16).expect("GPU 包络应成功");
    let cpu = envelope_minmax_cpu(&values, 16);
    assert_envelope_eq("全NaN", &gpu, &cpu);
    assert!(gpu.count.iter().all(|&c| c == 0));

    // ±inf 输入: 有序键含 ±inf, 往返后位级一致
    let values = vec![f32::INFINITY, f32::NEG_INFINITY, 1.0, -1.0];
    let gpu = envelope_minmax(&ctx, &values, 2).expect("GPU 包络应成功");
    let cpu = envelope_minmax_cpu(&values, 2);
    assert_envelope_eq("±inf", &gpu, &cpu);
}

#[test]
fn gpu_is_deterministic() {
    let Some(ctx) = GpuContext::acquire() else {
        eprintln!("无 GPU 适配器, 跳过 GPU 包络确定性测试");
        return;
    };
    let values = waveform(100_000);
    let run1 = envelope_minmax(&ctx, &values, 1024).expect("GPU 包络应成功");
    let run2 = envelope_minmax(&ctx, &values, 1024).expect("GPU 包络应成功");
    assert_eq!(run1, run2, "GPU 包络两次运行应完全一致");
}
