//! 图提交辅助路径基准 — 派生端口表计算与 ProtocolSource 注入
//!
//! 场景:
//! - `compute_derived`: N ∈ {50, 500} widget 混合节点 (Input/Math 各半) 的
//!   输出端口表派生 — 前端每次图提交全量调用
//! - `inject_protocol_sources`: 全局 Protocol 引用边的 ProtocolSource NodeDef
//!   注入 (编译前置步骤, 含去重与端口名推导)
//!
//! 说明: apply_tab_graph 全量提交 (含 AppState 写入与异步编译排队) 不在此测,
//! 由 cmd/graph 集成测试覆盖; 编译本身耗时由 engine compile_bench 单独度量。

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use graph_ops::{compute_derived, inject_protocol_sources};
use kind::{MathOp, NodeDef};
use testkit::{edge, make_input, make_math, make_protocol, make_protocol_source};

/// N 个 widget 节点 (Input/Math 交替)
fn widget_nodes(n: usize) -> Vec<NodeDef> {
    (0..n)
        .map(|i| {
            if i % 2 == 0 {
                make_input(&format!("w{i}"), "t0")
            } else {
                make_math(&format!("w{i}"), "t0", MathOp::Add, 1)
            }
        })
        .collect()
}

/// proto1 (全局 Protocol) + ps1 (ProtocolSource) + N widget
fn mixed_nodes(n: usize) -> Vec<NodeDef> {
    let mut nodes = vec![
        make_protocol("proto1"),
        make_protocol_source("ps1", "t0", "proto1", 4),
    ];
    nodes.extend(widget_nodes(n));
    nodes
}

fn bench_graph_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph_ops");
    for n in [50, 500] {
        group.throughput(Throughput::Elements(u64::try_from(n).unwrap_or(1)));
        let nodes = mixed_nodes(n);

        group.bench_function(format!("compute_derived_n{n}"), |b| {
            b.iter(|| black_box(compute_derived(&nodes)));
        });

        // 4 条全局 Protocol → widget 引用边 (注入按边起点扫描 + 去重)
        let edges: Vec<buffer_graph::Edge> = (0..4)
            .map(|i| {
                edge(
                    &format!("e{i}"),
                    "proto1",
                    &format!("ch{i}"),
                    &format!("w{}", i * 2),
                    "in0",
                )
            })
            .collect();
        group.bench_function(format!("inject_sources_n{n}"), |b| {
            b.iter(|| black_box(inject_protocol_sources(&nodes, &edges)));
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .output_directory(std::path::Path::new("../../../target/criterion/graph_ops"))
        .warm_up_time(std::time::Duration::from_secs(1))
        .measurement_time(std::time::Duration::from_secs(2))
        .sample_size(20);
    targets = bench_graph_ops
}
criterion_main!(benches);
