//! SciRS2 SIMD 批量求值等价性回归 — eval_simd on/off 的可观测输出**逐位一致**。
//!
//! 覆盖图集 (同源 pt, 三 tab, eval_workers = 4 多桶):
//! - t1 混合图: Math 链 (SIMD 单元) + Filter 状态链 (标量单元) + Trigger/Str
//!   (标量单元) — 批量/逐帧混合拓扑
//! - t3 超越函数图: Sin→Cos 链 + Log (SIMD 单元, 标量超越 — 位级一致)
//! - t5 边缘值图: Div (除零→0) / Sqrt (负数→0) / Min/Max/Avg (NaN 过滤)
//!
//! 帧流注入 NaN (每 17/23 帧) — NaN 过滤与全 NaN 空集语义全程受测。
//! 断言: DataBus 样本序列 / 派生环 / output_snapshot / source_frames 全部
//! 位级一致 (f64/f32 bits); 5000 帧跨块 (EVAL_CHUNK) 分块推进; SIMD 路径
//! 连跑两次验证确定性。

use std::collections::HashMap;

use app_state::AppState;
use data_bus::TopicKey;
use data_plane::frame_dispatch;
use dsp_filter::FilterConfig;
use engine::CompiledGraph;
use kind::{MathOp, StrOp};
use testkit::{
    edge, make_filter, make_input, make_math, make_protocol_source, make_sink, make_str,
    make_text_input, make_trigger, trigger_rule,
};
use trigger::TriggerMatchType;
use vofa_core::{DataFrame, PipelineConfig};

/// t1 — 批量/标量混合图: Math 链 (SIMD 单元) + Filter+Trigger 链 (标量单元)
/// + Str 链 (标量单元)。注: trig 挂 m3 (m2 链保持纯 Math 可批量)。
fn mixed_graph() -> CompiledGraph {
    let nodes = vec![
        make_protocol_source("ps1", "t1", "pt", 4),
        make_input("knob1", "t1"),
        // SIMD 单元: ps.ch0 × ps.ch1 + knob1
        make_math("m1", "t1", MathOp::Mul, 2),
        make_math("m2", "t1", MathOp::Add, 2),
        // 标量单元: 低通 (状态) + ps.ch3
        make_filter(
            "f1",
            "t1",
            FilterConfig::Lowpass {
                cutoff: 100.0,
                sample_rate: 1000.0,
            },
        ),
        make_math("m3", "t1", MathOp::Add, 2),
        // 标量单元: 字符串平面 → 数值 + Trigger (manual)
        make_text_input("textin", "t1", "abcd"),
        make_str("slen", "t1", StrOp::Len),
        make_math("m4", "t1", MathOp::Add, 2),
        make_trigger(
            "trig",
            "t1",
            "manual",
            "rising",
            "GO",
            vec![trigger_rule(
                "r1",
                TriggerMatchType::Exact,
                "GO",
                "number",
                42.0,
                "",
            )],
        ),
        make_sink("sinkA", "t1"),
        make_sink("sinkB", "t1"),
        make_sink("sinkC", "t1"),
        make_sink("sinkD", "t1"),
    ];
    let edges = vec![
        edge("e1", "ps1", "ch0", "m1", "in0"),
        edge("e2", "ps1", "ch1", "m1", "in1"),
        edge("e3", "m1", "result", "m2", "in0"),
        edge("e4", "knob1", "value", "m2", "in1"),
        edge("e5", "ps1", "ch2", "f1", "in0"),
        edge("e6", "f1", "result", "m3", "in0"),
        edge("e7", "ps1", "ch3", "m3", "in1"),
        edge("e8", "textin", "str", "slen", "str"),
        edge("e9", "slen", "result", "m4", "in0"),
        edge("e10", "ps1", "ch2", "m4", "in1"),
        edge("e11", "m3", "result", "trig", "trigger"),
        edge("e12", "m2", "result", "sinkA", "value"),
        edge("e13", "m3", "result", "sinkB", "value"),
        edge("e14", "m4", "result", "sinkC", "value"),
        edge("e15", "trig", "value", "sinkD", "value"),
    ];
    CompiledGraph::compile("t1".into(), nodes, edges).unwrap()
}

/// t3 — 超越函数图: Sin→Cos 链 + Log (NaN 注入受空集语义)
fn transcendent_graph() -> CompiledGraph {
    let nodes = vec![
        make_protocol_source("ps3", "t3", "pt", 4),
        make_math("t_sin", "t3", MathOp::Sin, 1),
        make_math("t_cos", "t3", MathOp::Cos, 1),
        make_math("t_log", "t3", MathOp::Log, 1),
        make_sink("sinkT1", "t3"),
        make_sink("sinkT2", "t3"),
    ];
    let edges = vec![
        edge("g1", "ps3", "ch0", "t_sin", "in0"),
        edge("g2", "t_sin", "result", "t_cos", "in0"),
        edge("g3", "ps3", "ch1", "t_log", "in0"),
        edge("g4", "t_cos", "result", "sinkT1", "value"),
        edge("g5", "t_log", "result", "sinkT2", "value"),
    ];
    CompiledGraph::compile("t3".into(), nodes, edges).unwrap()
}

/// t5 — 边缘值图: Div (除零→0) / Sqrt (负→0) / Min/Max/Avg (NaN 过滤)
fn edge_graph() -> CompiledGraph {
    let nodes = vec![
        make_protocol_source("ps5", "t5", "pt", 4),
        make_math("x_div", "t5", MathOp::Div, 2),
        make_math("x_sqrt", "t5", MathOp::Sqrt, 1),
        make_math("x_min", "t5", MathOp::Min, 2),
        make_math("x_max", "t5", MathOp::Max, 2),
        make_math("x_avg", "t5", MathOp::Avg, 2),
        make_sink("sinkX1", "t5"),
        make_sink("sinkX2", "t5"),
        make_sink("sinkX3", "t5"),
    ];
    let edges = vec![
        edge("h1", "ps5", "ch1", "x_div", "in0"),
        edge("h2", "ps5", "ch3", "x_div", "in1"),
        edge("h3", "ps5", "ch2", "x_sqrt", "in0"),
        edge("h4", "ps5", "ch0", "x_min", "in0"),
        edge("h5", "ps5", "ch1", "x_min", "in1"),
        edge("h6", "ps5", "ch0", "x_max", "in0"),
        edge("h7", "ps5", "ch2", "x_max", "in1"),
        edge("h8", "ps5", "ch1", "x_avg", "in0"),
        edge("h9", "ps5", "ch3", "x_avg", "in1"),
        edge("h10", "x_div", "result", "sinkX1", "value"),
        edge("h11", "x_sqrt", "result", "sinkX2", "value"),
        edge("h12", "x_min", "result", "sinkX3", "value"),
    ];
    CompiledGraph::compile("t5".into(), nodes, edges).unwrap()
}

fn install(state: &AppState) {
    let mut graphs = state.data_plane.eval.graphs.lock();
    graphs.insert("t1".into(), mixed_graph());
    graphs.insert("t3".into(), transcendent_graph());
    graphs.insert("t5".into(), edge_graph());
    drop(graphs);
    state
        .data_plane
        .eval
        .input_values
        .write()
        .insert("knob1".into(), 2.0);
}

fn set_config(state: &AppState, simd: bool) {
    *state.data_plane.pipeline_config.write() = PipelineConfig {
        eval_workers: 4,
        eval_simd: simd,
        ..PipelineConfig::default()
    };
}

type BusRx = tokio::sync::broadcast::Receiver<std::sync::Arc<data_bus::SampleBatch>>;

const TOPICS: [(&str, &str); 10] = [
    ("m2", "result"),
    ("m3", "result"),
    ("m4", "result"),
    ("trig", "value"),
    ("t_cos", "result"),
    ("t_log", "result"),
    ("x_div", "result"),
    ("x_sqrt", "result"),
    ("x_min", "result"),
    ("x_avg", "result"),
];

async fn subscribe_all(state: &AppState) -> HashMap<String, BusRx> {
    let mut rx = HashMap::new();
    for (node, port) in TOPICS {
        let receiver = state
            .data_plane
            .eval
            .data_bus
            .subscribe(TopicKey::new(node, port), 8192)
            .await
            .expect("订阅主题应存在");
        rx.insert(format!("{node}/{port}"), receiver);
    }
    rx
}

async fn drain_bus(rx: &mut HashMap<String, BusRx>) -> HashMap<String, Vec<f64>> {
    let mut bus = HashMap::new();
    for (name, receiver) in rx.iter_mut() {
        let mut values = Vec::new();
        let mut quiet = 0;
        loop {
            match receiver.try_recv() {
                Ok(batch) => {
                    quiet = 0;
                    for s in batch.samples.iter() {
                        values.push(s.value);
                    }
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
                    quiet += 1;
                    if quiet > 6 {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
                Err(_) => break,
            }
        }
        bus.insert(name.clone(), values);
    }
    bus
}

/// NaN 参与比较 → 一律按位比较 (f64)
fn assert_bits_eq(name: &str, a: &[f64], b: &[f64]) {
    assert_eq!(a.len(), b.len(), "{name} 点数一致");
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        assert!(
            x.to_bits() == y.to_bits(),
            "{name}[{i}]: {x} vs {y} 位级不一致"
        );
    }
}

struct Observed {
    bus: HashMap<String, Vec<f64>>,
    snapshot: engine::ValuesMap,
    derived: HashMap<String, Vec<f32>>,
    source_frame: Option<Vec<f32>>,
}

async fn observe(state: &AppState, rx: &mut HashMap<String, BusRx>) -> Observed {
    let bus = drain_bus(rx).await;
    let buf = state.data_plane.buffer_for("pt");
    let mut b = buf.lock();
    let mut derived = HashMap::new();
    for (sink, source, handle) in [
        ("sinkA", "m2", "result"),
        ("sinkT1", "t_cos", "result"),
        ("sinkT2", "t_log", "result"),
        ("sinkX1", "x_div", "result"),
        ("sinkX2", "x_sqrt", "result"),
        ("sinkX3", "x_min", "result"),
    ] {
        let idx = b.derived_port_index_of(sink, source, handle);
        derived.insert(format!("{sink}/{source}"), b.get_derived(idx, 1_000_000));
    }
    Observed {
        bus,
        snapshot: state.data_plane.eval.output_snapshot.lock().values.clone(),
        derived,
        source_frame: state
            .data_plane
            .eval
            .source_frames
            .lock()
            .get("pt")
            .map(|f| f.channels.clone()),
    }
}

/// 5000 帧批次 — 跨 2 块 (EVAL_CHUNK = 4096); ch1/ch2/ch3 注入 NaN 与 0
fn big_batch() -> Vec<DataFrame> {
    (0..5000)
        .map(|i| {
            let f = f32::from(u16::try_from(i % 256).unwrap_or(0)) - 128.0;
            let ch1 = if i % 17 == 0 { f32::NAN } else { f.abs() + 0.5 };
            let ch2 = if i % 23 == 0 { f32::NAN } else { f * 0.5 };
            let ch3 = if i % 29 == 0 { 0.0 } else { f + 3.0 };
            DataFrame::with_timestamp(u64::try_from(i).unwrap_or(0) * 100, vec![f, ch1, ch2, ch3])
        })
        .collect()
}

async fn run_fixture(simd: bool) -> Observed {
    let state = AppState::new();
    install(&state);
    set_config(&state, simd);
    let mut rx = subscribe_all(&state).await;
    frame_dispatch::on_frames(&state.data_plane, "pt", &big_batch());
    observe(&state, &mut rx).await
}

#[tokio::test]
async fn simd_matches_scalar_full_pipeline_bitwise() {
    let scalar = run_fixture(false).await;
    let simd = run_fixture(true).await;

    // DataBus 样本序列 — 全部主题位级一致 (含超越函数: 标量超越无近似)
    for topic in TOPICS.map(|(n, p)| format!("{n}/{p}")) {
        let (sv, gv) = (
            scalar.bus.get(&topic).unwrap(),
            simd.bus.get(&topic).unwrap(),
        );
        assert_bits_eq(&topic, sv, gv);
    }

    // 派生环 — 位级一致 + 点数一致
    for key in [
        "sinkA/m2",
        "sinkT1/t_cos",
        "sinkT2/t_log",
        "sinkX1/x_div",
        "sinkX2/x_sqrt",
        "sinkX3/x_min",
    ] {
        let (s, g) = (
            scalar.derived.get(key).unwrap(),
            simd.derived.get(key).unwrap(),
        );
        assert_eq!(s.len(), g.len(), "派生环 {key} 点数一致");
        for (i, (a, b)) in s.iter().zip(g.iter()).enumerate() {
            assert!(
                a.to_bits() == b.to_bits(),
                "派生环 {key}[{i}]: {a} vs {b} 位级不一致"
            );
        }
    }

    // 快照 — 全部 Math 节点位级一致
    let get =
        |o: &Observed, node: &str| o.snapshot.get(node).and_then(|m| m.get("result")).copied();
    for node in ["m2", "t_cos", "t_log", "x_div", "x_sqrt", "x_min", "x_avg"] {
        let (a, b) = (
            get(&scalar, node).expect("标量快照有值"),
            get(&simd, node).expect("simd 快照有值"),
        );
        assert!(
            a.to_bits() == b.to_bits(),
            "快照 {node}: {a} vs {b} 位级不一致"
        );
    }

    // source_frames 批尾缓存一致
    assert_eq!(scalar.source_frame, simd.source_frame);
}

#[tokio::test]
async fn simd_path_is_deterministic() {
    let run1 = run_fixture(true).await;
    let run2 = run_fixture(true).await;
    for topic in TOPICS.map(|(n, p)| format!("{n}/{p}")) {
        let (a, b) = (run1.bus.get(&topic).unwrap(), run2.bus.get(&topic).unwrap());
        assert_bits_eq(&topic, a, b);
    }
    for key in run1.derived.keys() {
        let (a, b) = (&run1.derived[key], &run2.derived[key]);
        assert_eq!(a.len(), b.len(), "{key} 点数一致");
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            assert!(x.to_bits() == y.to_bits(), "派生环 {key}[{i}] 确定性破坏");
        }
    }
}
