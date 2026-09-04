//! 数值平面评估基准 — 串行 (eval_workers=1) vs 图内路径并行 (≥2, SIMD on/off)
//!
//! 场景:
//! - `one_chain`: 单链图 (ProtocolSource+Input→Math×2), 10k 帧
//! - `eight_paths`: 单图 8 条独立 Math 链
//! - `deep8`: 8 条 6 级深链
//! - `filter4`: 4 条 Filter 状态链
//! - 每种图比较 workers=1/2/4 和 workers=4 + SIMD
//!
//! 说明: 基准不订阅 DataBus 主题 (端口 staging 路径由等价性测试覆盖),
//! 度量原始记录 + 图评估；这些图不含波形/频谱 sink，不代表派生写入成本。
//! 含真实波形 sink 和订阅的并发负载由 pipeline_soak 测量。
//! SIMD 开关两侧的逐位等价性由 `tests/graph_eval_simd_equiv.rs` 仲裁。

#![allow(clippy::cast_precision_loss)] // LCG 伪随机帧流: 小幅值整型 → f32 有意截断
#![allow(clippy::needless_borrow)]
use std::hint::black_box;

use app_state::AppState;
use criterion::{criterion_group, criterion_main, Criterion};
use data_plane::frame_dispatch;
use dsp_filter::FilterConfig;
use engine::CompiledGraph;
use kind::MathOp;
use testkit::{edge, make_filter, make_input, make_math, make_protocol_source};
use vofa_core::{DataFrame, PipelineConfig};

/// 确定性伪随机帧流 (LCG)
fn frames(count: usize, channels: usize) -> Vec<DataFrame> {
    let mut seed = 0x12345678_u32;
    (0..count)
        .map(|i| {
            let ch: Vec<f32> = (0..channels)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let v = i32::try_from((seed >> 8) % 10_000).unwrap_or(0);
                    (v % 500 - 250) as f32 / 10.0
                })
                .collect();
            DataFrame::with_timestamp(u64::try_from(i).unwrap_or(0) * 100, ch)
        })
        .collect()
}

fn setup(app: &AppState, graphs: Vec<CompiledGraph>) {
    {
        let mut g = app.data_plane.eval.graphs.lock();
        for (i, graph) in graphs.into_iter().enumerate() {
            g.insert(format!("t{i}"), graph);
        }
    }
    app.data_plane
        .eval
        .input_values
        .write()
        .insert("knob1".into(), 2.0);
}

/// 单链基线图
fn one_chain() -> CompiledGraph {
    let nodes = vec![
        make_protocol_source("ps1", "t0", "pt", 4),
        make_input("knob1", "t0"),
        make_math("m1", "t0", MathOp::Mul, 2),
        make_math("m2", "t0", MathOp::Add, 2),
    ];
    let edges = vec![
        edge("e1", "ps1", "ch0", "m1", "in0"),
        edge("e2", "ps1", "ch1", "m1", "in1"),
        edge("e3", "m1", "result", "m2", "in0"),
        edge("e4", "knob1", "value", "m2", "in1"),
    ];
    CompiledGraph::compile("t0".into(), nodes, edges).unwrap()
}

/// 单图 N 条独立链 (ps.ch0 → m{i} → s{i})
fn multi_path(paths: usize) -> CompiledGraph {
    let mut nodes = vec![make_protocol_source("ps1", "t0", "pt", paths.max(4))];
    let mut edges = Vec::new();
    for i in 0..paths {
        let id = format!("m{i}");
        nodes.push(make_math(&id, "t0", MathOp::Add, 2));
        edges.push(edge(&format!("e{i}a"), "ps1", "ch0", &id, "in0"));
        // 独立常量输入链尾 (Add 第二输入缺省 0.0 — 与前端未接线一致)
    }
    CompiledGraph::compile("t0".into(), nodes, edges).unwrap()
}

/// 单图 N 条 6 级深链 (ps.ch0 → c0 → c1 → ... → c5) — 真实 widget 图形态:
/// 计算为主、供给占比低 (并行加速的目标形态)
fn deep_chains(paths: usize, depth: usize) -> CompiledGraph {
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
    CompiledGraph::compile("t0".into(), nodes, edges).unwrap()
}

/// N 条 Filter 状态链 (ps.ch0 → filter{i} → math{i})
fn filter_heavy(chains: usize) -> CompiledGraph {
    let mut nodes = vec![make_protocol_source("ps1", "t0", "pt", chains.max(4))];
    let mut edges = Vec::new();
    for i in 0..chains {
        let fid = format!("f{i}");
        let mid = format!("m{i}");
        nodes.push(make_filter(
            &fid,
            "t0",
            FilterConfig::Lowpass {
                cutoff: 500.0,
                sample_rate: 48_000.0,
            },
        ));
        nodes.push(make_math(&mid, "t0", MathOp::Add, 1));
        edges.push(edge(&format!("e{i}a"), "ps1", "ch0", &fid, "in0"));
        edges.push(edge(&format!("e{i}b"), &fid, "result", &mid, "in0"));
    }
    CompiledGraph::compile("t0".into(), nodes, edges).unwrap()
}

fn multi_path_8() -> CompiledGraph {
    multi_path(8)
}
fn deep8() -> CompiledGraph {
    deep_chains(8, 6)
}
fn filter4() -> CompiledGraph {
    filter_heavy(4)
}

fn bench_eval(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph_eval");
    group.throughput(criterion::Throughput::Elements(10_000));
    for (name, make_graph) in [
        ("one_chain", one_chain as fn() -> CompiledGraph),
        ("eight_paths", multi_path_8 as fn() -> CompiledGraph),
        ("deep8", deep8 as fn() -> CompiledGraph),
        ("filter4", filter4 as fn() -> CompiledGraph),
    ] {
        for (workers, simd) in [(1, false), (2, false), (4, false), (4, true)] {
            let state = AppState::new();
            setup(&state, vec![make_graph()]);
            *state.data_plane.pipeline_config.write() = PipelineConfig {
                eval_workers: workers,
                eval_simd: simd,
                ..PipelineConfig::default()
            };
            let mut batch = frames(10_000, 4);
            let mut epoch = 0_u64;
            group.bench_function(format!("{name}_w{workers}_simd{simd}"), |b| {
                // 构造/推进时间轴不计入求值；每轮严格递增，避免污染环和状态算子。
                b.iter_custom(|iterations| {
                    let mut elapsed = std::time::Duration::ZERO;
                    for _ in 0..iterations {
                        for (i, frame) in batch.iter_mut().enumerate() {
                            frame.timestamp = epoch + i as u64 * 100;
                        }
                        epoch += batch.len() as u64 * 100;
                        let start = std::time::Instant::now();
                        black_box(frame_dispatch::on_frames(
                            &state.data_plane,
                            "pt",
                            black_box(&batch),
                        ));
                        elapsed += start.elapsed();
                    }
                    elapsed
                });
            });
        }
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .output_directory(std::path::Path::new("../../../target/criterion/graph_eval"))
        .warm_up_time(std::time::Duration::from_secs(1))
        .measurement_time(std::time::Duration::from_secs(2))
        .sample_size(20);
    targets = bench_eval
}
criterion_main!(benches);
