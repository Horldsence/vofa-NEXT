//! 节点图边集合基准 — 增删 / 索引重建 / 帧路由 / 环检测
//!
//! 场景:
//! - `graph_edges`: N ∈ {8, 64, 512} 条边 — `update_edges` 全量替换
//!   (含 rebuild_index)、`add_edge` 逐条插入、`remove_edge` 逐条删除
//!   (retain + 全量重建, O(n²) 形态)、`has_cycle` 无环链图 (逐边 DFS 重扫最坏形态)
//! - `graph_route`: 4 通道帧 `route_frame` 单源 ch0..ch3 fan-out 到 N 目标、
//!   `route_value` 单值推送命中 N 条边
//!
//! 说明: 只度量 buffer_graph 自身的边集合数据结构; 字节平面端到端路由
//! (route_bytes) 由 data_plane ingest_bench 覆盖, HIR 类型化图编译由
//! engine compile_bench 覆盖。

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use vofa_core::DataFrame;

use buffer_graph::{Edge, NodeGraph};

/// ps1.ch{0..3} → t{i}.in0 fan-out 边表 (N 条, 同源 4 通道)
fn fanout_edges(n: usize) -> Vec<Edge> {
    (0..n)
        .map(|i| Edge {
            id: format!("e{i}"),
            source: "ps1".to_string(),
            source_handle: format!("ch{}", i % 4),
            target: format!("t{i}"),
            target_handle: "in0".to_string(),
        })
        .collect()
}

/// n0 → n1 → ... → n{N} 链式边表 (无环, has_cycle 逐边 DFS 重扫的最坏形态)
fn chain_edges(n: usize) -> Vec<Edge> {
    (0..n)
        .map(|i| Edge {
            id: format!("e{i}"),
            source: format!("n{i}"),
            source_handle: "result".to_string(),
            target: format!("n{}", i + 1),
            target_handle: "in0".to_string(),
        })
        .collect()
}

fn bench_edges(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph_edges");
    for n in [8, 64, 512] {
        group.throughput(Throughput::Elements(u64::try_from(n).unwrap_or(1)));
        let fanout = fanout_edges(n);
        let chain = chain_edges(n);

        // 全量替换: 调用方传入新边表, 计时仅含替换 + 索引重建
        group.bench_function(format!("update_edges_n{n}"), |b| {
            let mut g = NodeGraph::new();
            b.iter_batched(
                || fanout.clone(),
                |edges| {
                    g.update_edges(edges);
                    black_box(g.edges().len())
                },
                BatchSize::SmallInput,
            );
        });

        // 逐条插入: 增量维护索引 (无全量重建)
        group.bench_function(format!("add_edge_n{n}"), |b| {
            b.iter(|| {
                let mut g = NodeGraph::new();
                for e in &fanout {
                    g.add_edge(e.clone());
                }
                black_box(g.edges().len())
            });
        });

        // 逐条删除: 每条 retain + 全量重建 — 总成本 O(n²), 预期随 N 显著恶化
        group.bench_function(format!("remove_edge_n{n}"), |b| {
            b.iter_batched(
                || {
                    let mut g = NodeGraph::new();
                    g.update_edges(fanout.clone());
                    g
                },
                |mut g| {
                    for i in 0..n {
                        g.remove_edge(&format!("e{i}"));
                    }
                    black_box(g.edges().len())
                },
                BatchSize::SmallInput,
            );
        });

        // 无环链图上的环检测 (返回 false 的最坏路径)
        let mut chain_graph = NodeGraph::new();
        chain_graph.update_edges(chain);
        group.bench_function(format!("has_cycle_n{n}"), |b| {
            b.iter(|| black_box(chain_graph.has_cycle()));
        });
    }
    group.finish();
}

fn bench_route(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph_route");
    for n in [8, 64, 512] {
        group.throughput(Throughput::Elements(u64::try_from(n).unwrap_or(1)));
        let mut g = NodeGraph::new();
        g.update_edges(fanout_edges(n));
        let frame = DataFrame::with_timestamp(0, vec![1.0, -2.0, 3.5, 0.25]);

        // 4 通道帧 fan-out: 命中 n 条边, 每条克隆 target/handle 字符串
        group.bench_function(format!("route_frame_n{n}"), |b| {
            b.iter(|| black_box(g.route_frame(&frame)));
        });

        // 单值推送: 命中同源全部 n 条边
        group.bench_function(format!("route_value_n{n}"), |b| {
            b.iter(|| black_box(g.route_value("ps1", 1.5)));
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .output_directory(std::path::Path::new("../../../target/criterion/node_graph"))
        .warm_up_time(std::time::Duration::from_secs(1))
        .measurement_time(std::time::Duration::from_secs(2))
        .sample_size(20);
    targets = bench_edges, bench_route
}
criterion_main!(benches);
