//! 数值平面评估基准 — 串行 (eval_workers=1) vs 图内路径并行 (≥2, SIMD on/off)
//!
//! 场景:
//! - `serial_baseline_1chain`: 单链图 (ProtocolSource+Input→Math×2), 10k 帧
//! - `parallel_8paths_w2/w4`: 单图 8 条独立 Math 链 (路径级并行的目标形态)
//! - `deep8_chains_w2/w4`: 8 条 6 级深链 (计算为主)
//! - `filter_heavy_w1/w4`: 4 条 Filter 状态链 (有状态切分正确性下的吞吐)
//! - `simd_*`: 同图集 eval_simd on/off 对比 (Math 资格单元批量求值收益)
//!
//! 说明: 基准不订阅 DataBus 主题 (端口 staging 路径由等价性测试覆盖),
//! 度量的是 push_frame + 图评估 + 派生回放 + 频谱回放的主路径耗时。
//! SIMD 开关两侧的逐位等价性由 `tests/graph_eval_simd_equiv.rs` 仲裁。

#![allow(clippy::cast_precision_loss)] // LCG 伪随机帧流: 小幅值整型 → f32 有意截断
#![allow(clippy::needless_borrow)]
use std::hint::black_box;

use app_state::AppState;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
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

fn set_workers(app: &AppState, n: usize) {
    *app.data_plane.pipeline_config.write() = PipelineConfig {
        eval_workers: n,
        ..PipelineConfig::default()
    };
}

fn set_simd(app: &AppState, enabled: bool) {
    *app.data_plane.pipeline_config.write() = PipelineConfig {
        eval_workers: 4,
        eval_simd: enabled,
        ..PipelineConfig::default()
    };
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
    let frames = frames(10_000, 4);
    let mut group = c.benchmark_group("graph_eval");
    group.throughput(criterion::Throughput::Elements(frames.len() as u64));

    // 串行基线: 单链
    group.bench_function("serial_baseline_1chain_w1", |b| {
        let state = AppState::new();
        setup(&state, vec![one_chain()]);
        set_workers(&state, 1);
        b.iter_batched(
            || (),
            |()| {
                let _ = black_box(frame_dispatch::on_frames(
                    &state.data_plane,
                    "pt",
                    black_box(frames.as_slice()),
                ));
            },
            BatchSize::SmallInput,
        );
    });

    // 8 独立路径: 串行 vs 并行 2/4
    for workers in [1usize, 2, 4] {
        group.bench_function(format!("parallel_8paths_w{workers}"), |b| {
            let state = AppState::new();
            setup(&state, vec![multi_path(8)]);
            set_workers(&state, workers);
            b.iter_batched(
                || (),
                |()| {
                    let _ = black_box(frame_dispatch::on_frames(
                        &state.data_plane,
                        "pt",
                        black_box(frames.as_slice()),
                    ));
                },
                BatchSize::SmallInput,
            );
        });
    }

    // 8 条 6 级深链 (计算为主): 串行 vs 并行 2/4
    for workers in [1usize, 2, 4] {
        group.bench_function(format!("deep8_chains_w{workers}"), |b| {
            let state = AppState::new();
            setup(&state, vec![deep_chains(8, 6)]);
            set_workers(&state, workers);
            b.iter_batched(
                || (),
                |()| {
                    let _ = black_box(frame_dispatch::on_frames(
                        &state.data_plane,
                        "pt",
                        black_box(frames.as_slice()),
                    ));
                },
                BatchSize::SmallInput,
            );
        });
    }

    // Filter 重图: 串行 vs 并行 4
    for workers in [1usize, 4] {
        group.bench_function(format!("filter_heavy_4chains_w{workers}"), |b| {
            let state = AppState::new();
            setup(&state, vec![filter_heavy(4)]);
            set_workers(&state, workers);
            b.iter_batched(
                || (),
                |()| {
                    let _ = black_box(frame_dispatch::on_frames(
                        &state.data_plane,
                        "pt",
                        black_box(frames.as_slice()),
                    ));
                },
                BatchSize::SmallInput,
            );
        });
    }

    // SIMD 批量组: 同图集 w4 × eval_simd on/off 对比 (Math 资格单元 SoA 批量)
    // - 1chain / 8paths / deep8: Math 单元全部 SIMD 资格
    // - filter_heavy: Math 单元无资格输入 (Filter 有状态) → 全标量 (度量判定开销)
    for (name, make_graph) in [
        ("simd_1chain", one_chain as fn() -> CompiledGraph),
        ("simd_8paths", multi_path_8 as fn() -> CompiledGraph),
        ("simd_deep8", deep8 as fn() -> CompiledGraph),
        ("simd_filter_heavy", filter4 as fn() -> CompiledGraph),
    ] {
        for enabled in [false, true] {
            let state = AppState::new();
            setup(&state, vec![make_graph()]);
            set_simd(&state, enabled);
            group.bench_function(
                format!("{name}_{}", if enabled { "on" } else { "off" }),
                |b| {
                    b.iter_batched(
                        || (),
                        |()| {
                            let _ = black_box(frame_dispatch::on_frames(
                                &state.data_plane,
                                "pt",
                                black_box(frames.as_slice()),
                            ));
                        },
                        BatchSize::SmallInput,
                    );
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_eval);
criterion_main!(benches);
