//! 真实 WWB1 编码与 JSON 快照编码的 CPU/载荷基准，不包含 IPC 传输。
use buffer_databuffer::DataBuffer;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use display::encode_waveform_window;
use std::{hint::black_box, path::Path, time::Duration};

fn bench_wire(c: &mut Criterion) {
    let mut group = c.benchmark_group("waveform_wire");
    for (channels, points) in [(4, 2_000), (4, 12_000), (16, 12_000)] {
        let mut buffer = DataBuffer::new(points, channels);
        for i in 0..points {
            let value = f32::from(u16::try_from(i % 100).unwrap()) * 0.017;
            buffer.push_frame_at(i as u64 * 2, &vec![value.sin(); channels]);
        }
        let window = buffer.get_recent(points);
        let bytes = encode_waveform_window(&window).len();
        eprintln!(
            "载荷 {channels}ch × {points}: WWB1={bytes} B, JSON={} B",
            serde_json::to_vec(&window).unwrap().len()
        );
        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("wwb1", format!("{channels}ch_{points}")),
            &window,
            |b, w| b.iter(|| black_box(encode_waveform_window(black_box(w)))),
        );
        group.throughput(Throughput::Bytes(
            serde_json::to_vec(&window).unwrap().len() as u64,
        ));
        group.bench_with_input(
            BenchmarkId::new("json", format!("{channels}ch_{points}")),
            &window,
            |b, w| b.iter(|| black_box(serde_json::to_vec(black_box(w)).unwrap())),
        );
    }
    group.finish();
}
criterion_group! {
    name = benches;
    config = Criterion::default().output_directory(Path::new("../../../target/criterion/waveform_wire"))
        .warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(3)).sample_size(30);
    targets = bench_wire
}
criterion_main!(benches);
