//! 字节平面完整摄入 benchmark：JustFloat 解码 → 采样时钟 → 原始记录 → 评估入队。
//! 目标门禁为持续吞吐 >10 MB/s；测试批大小与 read_task 的典型 64 KB 合批一致。

#![allow(clippy::cast_precision_loss)]

use std::hint::black_box;
use std::path::Path;
use std::time::Duration;

use app_state::AppState;
use buffer_graph::Edge;
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use data_plane::byte_router::route_bytes;
use data_plane::decoder_feed::DecoderFeedCache;
use engine::BytePlan;
use kind::{NodeDef, NodeKind};
use schema_types::ProtocolConfig;
use vofa_core::{TestDataConfig, TestSignal, TransportConfig};

const CHANNELS: usize = 4;
const FRAMES_PER_CHUNK: usize = 3_200;
const BYTES_PER_FRAME: usize = CHANNELS * 4 + 4;

fn setup_plane() -> data_plane::DataPlaneState {
    let state = AppState::new();
    let plane = state.data_plane;
    let nodes = vec![
        NodeDef {
            id: "tp".into(),
            tab_id: "bench".into(),
            kind: NodeKind::Transport {
                config: TransportConfig::TestData(TestDataConfig {
                    channels: CHANNELS,
                    sample_rate: 700_000.0,
                    signal: TestSignal::Sine,
                }),
            },
        },
        NodeDef {
            id: "pt".into(),
            tab_id: "bench".into(),
            kind: NodeKind::Protocol {
                config: ProtocolConfig::JustFloat {
                    channels: Some(CHANNELS),
                },
                convert_to: None,
                schema: None,
            },
        },
    ];
    let edges = vec![Edge {
        id: "tp-pt".into(),
        source: "tp".into(),
        source_handle: "rx".into(),
        target: "pt".into(),
        target_handle: "in".into(),
    }];
    {
        let mut global = plane.global_nodes.lock();
        for node in nodes {
            global.insert(node.id.clone(), node);
        }
        let typed = engine::TypedGraph::build(global.values().cloned(), edges).unwrap();
        *plane.byte_plan.lock() = BytePlan::build(&typed).unwrap();
    }
    plane.sync_protocol_states();
    plane
}

fn justfloat_chunk() -> Vec<u8> {
    let mut chunk = Vec::with_capacity(FRAMES_PER_CHUNK * BYTES_PER_FRAME);
    for frame in 0..FRAMES_PER_CHUNK {
        let phase = frame as f32 * 0.017;
        for channel in 0..CHANNELS {
            let value = (channel as f32)
                .mul_add(0.7, phase)
                .sin()
                .mul_add(125.0, 128.0);
            chunk.extend_from_slice(&value.to_le_bytes());
        }
        chunk.extend_from_slice(&[0x00, 0x00, 0x80, 0x7f]);
    }
    chunk
}

fn bench_ingest(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    let plane = setup_plane();
    let chunk = justfloat_chunk();
    let mut cache = DecoderFeedCache::new();
    let mut group = c.benchmark_group("data_plane_ingest");
    group.throughput(Throughput::Bytes(chunk.len() as u64));
    group.bench_function("justfloat_4ch_64kb_parse_record_enqueue", |b| {
        b.iter(|| {
            let summary = runtime.block_on(route_bytes(
                &plane,
                None,
                "tp",
                black_box(&chunk),
                1_024,
                &mut cache,
                None,
            ));
            black_box(summary.frames);
        });
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .output_directory(Path::new("../../../target/criterion/data_plane_ingest"))
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(30);
    targets = bench_ingest
}
criterion_main!(benches);
