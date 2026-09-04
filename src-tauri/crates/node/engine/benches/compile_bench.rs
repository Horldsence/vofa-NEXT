//! 图编译流水线基准 — HIR 构建 → 平面投影 → lowering 全链与分段计时
//!
//! 图形状与 data_plane eval_bench 对齐 (one_chain / eight_paths / deep8 /
//! filter4), 便于与 graph_eval 端到端求值数字对照; 另加 `byte_chain`
//! (Transport→Protocol→ProtocolSource→Math) 覆盖 BytePlan 非空形态。
//!
//! 分段:
//! - `full`: `CompiledGraph::compile` 端到端 (前端每次拓扑改动触发的热路径,
//!   eval_bench 只在 setup 编译不计时 — 本基准补上该缺口)
//! - `typed`: `TypedGraph::build` (interning + 域解析 + 边分类)
//! - `value_plane`: 值平面投影 + 拓扑排序
//! - `byte_plan`: 字节平面计划 (拓扑序 + consumers 聚合)
//! - `lower`: 槽位 lowering (含 units 连通分量切分)
//!
//! 说明: 慢路径 evaluate (逐节点 NodeArm 分派) 不在此测 — 槽位快路径由
//! eval eval_run_bench 覆盖, 端到端求值由 data_plane eval_bench 覆盖。

use std::hint::black_box;

use buffer_graph::Edge;
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use dsp_filter::FilterConfig;
use engine::CompiledGraph;
use hir::TypedGraph;
use kind::{MathOp, NodeDef};
use lower::lower_value_plane;
use plane::{value_plane, BytePlan};
use testkit::{
    edge, make_filter, make_input, make_math, make_protocol, make_protocol_source, make_transport,
};

/// 单链基线图: ps1(ch0,ch1) + knob1 → Mul → Add
fn one_chain() -> (Vec<NodeDef>, Vec<Edge>) {
    (
        vec![
            make_protocol_source("ps1", "t0", "pt", 4),
            make_input("knob1", "t0"),
            make_math("m1", "t0", MathOp::Mul, 2),
            make_math("m2", "t0", MathOp::Add, 2),
        ],
        vec![
            edge("e1", "ps1", "ch0", "m1", "in0"),
            edge("e2", "ps1", "ch1", "m1", "in1"),
            edge("e3", "m1", "result", "m2", "in0"),
            edge("e4", "knob1", "value", "m2", "in1"),
        ],
    )
}

/// 单图 N 条独立链: ps1.ch0 → m{i} (Add 第二输入缺省 0.0 — 与前端未接线一致)
fn multi_path(paths: usize) -> (Vec<NodeDef>, Vec<Edge>) {
    let mut nodes = vec![make_protocol_source("ps1", "t0", "pt", paths.max(4))];
    let mut edges = Vec::new();
    for i in 0..paths {
        let id = format!("m{i}");
        nodes.push(make_math(&id, "t0", MathOp::Add, 2));
        edges.push(edge(&format!("e{i}a"), "ps1", "ch0", &id, "in0"));
    }
    (nodes, edges)
}

/// 单图 N 条 6 级深链: ps1.ch0 → c{i}_0 → ... → c{i}_5 (并行加速的目标形态)
fn deep_chains(paths: usize, depth: usize) -> (Vec<NodeDef>, Vec<Edge>) {
    let mut nodes = vec![make_protocol_source("ps1", "t0", "pt", paths.max(4))];
    let mut edges = Vec::new();
    for i in 0..paths {
        let mut prev = ("ps1".to_string(), "ch0".to_string());
        for d in 0..depth {
            let id = format!("c{i}_{d}");
            nodes.push(make_math(&id, "t0", MathOp::Add, 2));
            edges.push(edge(&format!("e{i}_{d}"), &prev.0, &prev.1, &id, "in0"));
            prev = (id, "result".to_string());
        }
    }
    (nodes, edges)
}

/// N 条 Filter 状态链: ps1.ch0 → filter{i} → math{i}
fn filter_heavy(chains: usize) -> (Vec<NodeDef>, Vec<Edge>) {
    let mut nodes = vec![make_protocol_source("ps1", "t0", "pt", chains.max(4))];
    let mut edges = Vec::new();
    for i in 0..chains {
        let fid = format!("f{i}");
        let mid = format!("m{i}");
        nodes.push(make_math(&mid, "t0", MathOp::Add, 1));
        nodes.push(make_filter(
            &fid,
            "t0",
            FilterConfig::Lowpass {
                cutoff: 500.0,
                sample_rate: 48_000.0,
            },
        ));
        edges.push(edge(&format!("e{i}a"), "ps1", "ch0", &fid, "in0"));
        edges.push(edge(&format!("e{i}b"), &fid, "result", &mid, "in0"));
    }
    (nodes, edges)
}

/// 字节平面链: Transport(rx) → Protocol(in) + ProtocolSource.ch0 → Math
/// (BytePlan consumers 非空的唯一形态; 其余形状字节平面为空)
fn byte_chain() -> (Vec<NodeDef>, Vec<Edge>) {
    (
        vec![
            make_transport("tp1"),
            make_protocol("pt1"),
            make_protocol_source("ps1", "t0", "pt1", 4),
            make_math("m1", "t0", MathOp::Mul, 1),
        ],
        vec![
            edge("eb1", "tp1", "rx", "pt1", "in"),
            edge("ev1", "ps1", "ch0", "m1", "in0"),
        ],
    )
}

fn multi_path_8() -> (Vec<NodeDef>, Vec<Edge>) {
    multi_path(8)
}

fn deep8() -> (Vec<NodeDef>, Vec<Edge>) {
    deep_chains(8, 6)
}

fn filter4() -> (Vec<NodeDef>, Vec<Edge>) {
    filter_heavy(4)
}

fn bench_compile(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph_compile");
    for (name, build_shape) in [
        ("one_chain", one_chain as fn() -> (Vec<NodeDef>, Vec<Edge>)),
        (
            "eight_paths",
            multi_path_8 as fn() -> (Vec<NodeDef>, Vec<Edge>),
        ),
        ("deep8", deep8 as fn() -> (Vec<NodeDef>, Vec<Edge>)),
        ("filter4", filter4 as fn() -> (Vec<NodeDef>, Vec<Edge>)),
        (
            "byte_chain",
            byte_chain as fn() -> (Vec<NodeDef>, Vec<Edge>),
        ),
    ] {
        let (nodes, edges) = build_shape();
        let node_count = u64::try_from(nodes.len()).unwrap_or(1);
        group.throughput(Throughput::Elements(node_count));

        // 端到端编译
        group.bench_function(BenchmarkId::new("full", name), |b| {
            b.iter_batched(
                || (nodes.clone(), edges.clone()),
                |(n, e)| black_box(CompiledGraph::compile("t0".into(), n, e).unwrap()),
                BatchSize::SmallInput,
            );
        });

        // 分段 1: HIR 构建
        group.bench_function(BenchmarkId::new("typed", name), |b| {
            b.iter_batched(
                || (nodes.clone(), edges.clone()),
                |(n, e)| black_box(TypedGraph::build(n, e).unwrap()),
                BatchSize::SmallInput,
            );
        });

        // 分段 2/3/4: 投影与 lowering (输入为引用, 直接计时)
        let typed = TypedGraph::build(nodes.clone(), edges.clone()).unwrap();
        group.bench_function(BenchmarkId::new("value_plane", name), |b| {
            b.iter(|| black_box(value_plane(&typed).unwrap()));
        });
        group.bench_function(BenchmarkId::new("byte_plan", name), |b| {
            b.iter(|| black_box(BytePlan::build(&typed).unwrap()));
        });
        let mir = value_plane(&typed).unwrap();
        group.bench_function(BenchmarkId::new("lower", name), |b| {
            b.iter(|| black_box(lower_value_plane(&typed, &mir)));
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .output_directory(std::path::Path::new("../../../target/criterion/graph_compile"))
        .warm_up_time(std::time::Duration::from_secs(1))
        .measurement_time(std::time::Duration::from_secs(2))
        .sample_size(20);
    targets = bench_compile
}
criterion_main!(benches);
