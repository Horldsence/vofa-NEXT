//! 槽位求值微基准 — `CompiledEval::run` 逐帧评估 + `materialize` 快照物化
//!
//! 图形状与 data_plane eval_bench 对齐 (one_chain / deep8 / filter4), 每形状
//! 10k 帧批评估, 含每帧槽位清零协议 (slots/written/str 清零, 与 graph_eval
//! 调用方一致); 另测快照发布点的 materialize (全槽位扫描物化)。
//!
//! 对照: 与 graph_eval 端到端数字相减即为数据平面开销 (记录求值/锁/派生
//! 收集); filter 链的稳态滤波成本在 run 内复现 (DigitalFilter 状态跨帧持久)。
//!
//! 说明: 不含 DataPlaneState 锁、派生收集与波形/频谱 sink 写入 — 前者由
//! graph_eval 端到端差值体现, 后者由 pipeline_soak 覆盖。

use std::collections::HashMap;
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use dsp_filter::FilterConfig;
use eval::{CompiledEval, StringValuesMap, ValuesMap};
use hir::TypedGraph;
use kind::{MathOp, NodeDef};
use lower::lower_value_plane;
use plane::value_plane;
use testkit::{
    edge, empty_texts, make_filter, make_input, make_math, make_protocol_source, source_frames,
};

/// 节点/边 → 三段编译 → 槽位评估表
fn compile_eval(nodes: Vec<NodeDef>, edges: Vec<buffer_graph::Edge>) -> CompiledEval {
    let typed = TypedGraph::build(nodes, edges).expect("HIR 编译");
    let mir = value_plane(&typed).expect("值平面投影");
    CompiledEval::new(lower_value_plane(&typed, &mir))
}

/// 单链基线: ps1(ch0,ch1) + knob1 → Mul → Add
fn one_chain() -> CompiledEval {
    compile_eval(
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

/// 单图 N 条 6 级深链: ps1.ch0 → c{i}_0 → ... → c{i}_5
fn deep_chains(paths: usize, depth: usize) -> CompiledEval {
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
    compile_eval(nodes, edges)
}

/// N 条 Filter 状态链: ps1.ch0 → filter{i} → math{i} (状态跨帧持久)
fn filter_heavy(chains: usize) -> CompiledEval {
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
    compile_eval(nodes, edges)
}

fn deep8() -> CompiledEval {
    deep_chains(8, 6)
}

fn filter4() -> CompiledEval {
    filter_heavy(4)
}

const BATCH: usize = 10_000;

fn bench_run(c: &mut Criterion) {
    let mut group = c.benchmark_group("slot_eval");
    let frames = source_frames(&[("pt", vec![1.0, -2.0, 3.5, 0.25])]);
    let inputs: HashMap<String, f32> = HashMap::from([("knob1".to_string(), 2.0)]);

    for (name, eval) in [
        ("one_chain", one_chain()),
        ("deep8", deep8()),
        ("filter4", filter4()),
    ] {
        // 缓冲按槽位数分配, 跨帧复用 (与 graph_eval slot_bufs 一致)
        let mut slots = vec![0.0; eval.slot_count()];
        let mut written = vec![false; eval.slot_count()];
        let mut str_slots = vec![String::new(); eval.str_slot_count()];
        let mut str_written = vec![false; eval.str_slot_count()];
        let mut filter_states = HashMap::new();
        let decoder_states = HashMap::new();
        let mut ifft_states = HashMap::new();
        let mut trigger_states = HashMap::new();
        let customs: HashMap<String, HashMap<String, f32>> = HashMap::new();

        group.throughput(Throughput::Elements(u64::try_from(BATCH).unwrap_or(1)));
        group.bench_function(format!("{name}_run_10k"), |b| {
            b.iter(|| {
                for _ in 0..BATCH {
                    // 每帧清零协议 (见 CompiledEval::run 文档): 防上帧值泄漏
                    slots.fill(0.0);
                    written.fill(false);
                    str_slots.iter_mut().for_each(String::clear);
                    str_written.fill(false);
                    eval.run(
                        &frames,
                        &empty_texts(),
                        &inputs,
                        &customs,
                        &mut filter_states,
                        &decoder_states,
                        &mut ifft_states,
                        &mut trigger_states,
                        &mut slots,
                        &mut written,
                        &mut str_slots,
                        &mut str_written,
                    );
                }
                black_box(slots[0])
            });
        });

        // 预跑一帧置位 written — materialize 只物化本帧已产出槽位
        eval.run(
            &frames,
            &empty_texts(),
            &inputs,
            &customs,
            &mut filter_states,
            &decoder_states,
            &mut ifft_states,
            &mut trigger_states,
            &mut slots,
            &mut written,
            &mut str_slots,
            &mut str_written,
        );
        let mut values = ValuesMap::default();
        let mut str_values = StringValuesMap::default();
        group.throughput(Throughput::Elements(
            u64::try_from(eval.slot_count()).unwrap_or(1),
        ));
        group.bench_function(format!("{name}_materialize"), |b| {
            b.iter(|| {
                values.clear();
                eval.materialize(&slots, &written, &mut values);
                str_values.clear();
                eval.materialize_str(&str_slots, &str_written, &mut str_values);
                black_box(values.len())
            });
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .output_directory(std::path::Path::new("../../../target/criterion/slot_eval"))
        .warm_up_time(std::time::Duration::from_secs(1))
        .measurement_time(std::time::Duration::from_secs(2))
        .sample_size(20);
    targets = bench_run
}
criterion_main!(benches);
