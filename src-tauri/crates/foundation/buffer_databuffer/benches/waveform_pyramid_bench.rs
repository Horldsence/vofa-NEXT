//! 长时间波形金字塔基准。
//!
//! 场景以 JustFloat 4 通道线缆帧（20 B/帧）折算吞吐，覆盖：
//! - L0 已滚动、L1/L2 持续级联时的记录写入；
//! - 320 万帧历史上的全局概览预算查询；
//! - 700 kS/s、最近 2 秒主图的预算查询。

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::hint::black_box;
use std::path::Path;
use std::time::Duration;

use buffer_databuffer::{DataBuffer, WaveformSeriesSelection};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

const CHANNELS: usize = 4;
const JUSTFLOAT_BYTES_PER_FRAME: u64 = (CHANNELS * 4 + 4) as u64;
const SAMPLE_RATE: f64 = 700_000.0;

fn values(index: u64) -> [f32; CHANNELS] {
    let phase = index as f32 * 0.017;
    std::array::from_fn(|channel| {
        (channel as f32)
            .mul_add(0.7, phase)
            .sin()
            .mul_add(125.0, 128.0)
    })
}

fn filled_buffer(frames: u64, capacity: usize) -> DataBuffer {
    let mut buffer = DataBuffer::new(capacity, CHANNELS);
    for index in 0..frames {
        let timestamp = (index as f64 * 1_000_000.0 / SAMPLE_RATE).round() as u64;
        buffer.push_frame_at(timestamp, &values(index));
    }
    buffer
}

fn bench_waveform_pyramid(c: &mut Criterion) {
    const WRITE_BATCH: u64 = 65_536;
    const HISTORY_FRAMES: u64 = 3_200_000;

    let mut write_group = c.benchmark_group("waveform_pyramid_write");
    write_group.throughput(Throughput::Bytes(WRITE_BATCH * JUSTFLOAT_BYTES_PER_FRAME));
    let mut write_buffer = filled_buffer(200_000, 100_000);
    let mut next_index = 200_000_u64;
    write_group.bench_function("4ch_l0_rolling_and_tier_folding", |b| {
        b.iter(|| {
            for index in next_index..next_index + WRITE_BATCH {
                let timestamp = (index as f64 * 1_000_000.0 / SAMPLE_RATE).round() as u64;
                write_buffer.push_frame_at(timestamp, black_box(&values(index)));
            }
            next_index += WRITE_BATCH;
        });
    });
    write_group.finish();

    let history = filled_buffer(HISTORY_FRAMES, 100_000);
    assert!(history.storage_overflow() > 0);
    let selection = WaveformSeriesSelection {
        channels: (0..CHANNELS).collect(),
        derived: vec![],
    };
    let mut query_group = c.benchmark_group("waveform_pyramid_query");
    query_group.bench_function("overview_3200k_to_2000", |b| {
        b.iter(|| {
            black_box(history.snapshot_all_budget(2_000).into_min_max(2_000));
        });
    });
    query_group.bench_function("detail_2s_700ksps_to_12000", |b| {
        b.iter(|| {
            black_box(
                history
                    .snapshot_window_budget(-2_000.0, 0.0, &selection, 12_000)
                    .into_min_max(12_000),
            );
        });
    });
    query_group.finish();

    let mut derived_group = c.benchmark_group("derived_recent_query");
    for capacity in [2_000, 100_000, 1_750_000] {
        let mut buffer = DataBuffer::new(capacity, CHANNELS);
        let writer = buffer.derived_writer();
        let indices: Vec<_> = (0..CHANNELS)
            .map(|ch| writer.port_index_of("wave", &format!("math{ch}"), "out"))
            .collect();
        for index in 0..capacity * 2 {
            let timestamp = index as u64 * 2;
            buffer.push_frame_at(timestamp, &values(index as u64));
            writer.append(indices.iter().map(|&idx| (idx, timestamp, index as f32)));
        }
        derived_group.bench_with_input(
            BenchmarkId::new("4derived_recent128", capacity),
            &buffer,
            |b, buffer| {
                b.iter(|| black_box(buffer.get_recent(128)));
            },
        );
    }
    derived_group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .output_directory(Path::new("../../../target/criterion/waveform_pyramid"))
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(30);
    targets = bench_waveform_pyramid
}
criterion_main!(benches);
