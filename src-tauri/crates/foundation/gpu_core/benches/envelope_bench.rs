//! 波形包络降采样基准 — GPU (wgpu) vs CPU 线性扫描
//!
//! 论证文档 (docs/wgpu-prerender-feasibility.md) 的实测数据来源:
//! 典型波形窗口量级 100k ~ 4M 点 (100k = DataBuffer 默认容量),
//! columns = 2048 (2K 宽度画布的逐像素列包络)。

#![allow(clippy::cast_precision_loss)] // LCG 伪随机数据流: 小幅值整型 → f32 有意截断

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use gpu_core::{envelope_minmax, envelope_minmax_cpu, GpuContext};

/// 确定性伪随机波形 (正弦 + LCG 噪声, 含少量 NaN)
fn waveform(n: usize) -> Vec<f32> {
    let mut seed = 0xfeed_beef_u32;
    (0..n)
        .map(|i| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = ((seed >> 8) % 2000) as f32 / 1000.0 - 1.0;
            if i % 997 == 0 {
                f32::NAN
            } else {
                ((i % 10_000) as f32 * 0.001 * std::f32::consts::TAU).sin() + noise * 0.1
            }
        })
        .collect()
}

fn bench_envelope(c: &mut Criterion) {
    let columns = 2048;
    let mut group = c.benchmark_group("waveform_envelope");
    for n in [100_000, 1_000_000, 4_000_000] {
        let values = waveform(n);
        group.throughput(criterion::Throughput::Elements(n as u64));
        group.bench_function(format!("cpu_{n}"), |b| {
            b.iter(|| black_box(envelope_minmax_cpu(black_box(&values), columns)));
        });
        let Some(ctx) = GpuContext::acquire() else {
            eprintln!("无 GPU 适配器, 跳过 gpu_{n} 组");
            continue;
        };
        group.bench_function(format!("gpu_{n}"), |b| {
            b.iter_batched(
                || (),
                |()| {
                    black_box(envelope_minmax(&ctx, black_box(&values), columns))
                        .expect("GPU 包络应成功");
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .output_directory(std::path::Path::new("../../../target/criterion/envelope"))
        .warm_up_time(std::time::Duration::from_secs(1))
        .measurement_time(std::time::Duration::from_secs(2))
        .sample_size(20);
    targets = bench_envelope
}
criterion_main!(benches);
