//! GPU/串行评估等价性回归 — wgpu 卸载路径 (eval_backend = gpu) 与
//! 串行热路径 (eval_backend = cpu, eval_workers = 1) 的可观测输出一致。
//!
//! 覆盖图集 (同源 pt, 双 tab):
//! - t1 精确算子图: Math 链 (Mul+Add, GPU 单元) + Filter 状态链 (CPU 单元)
//!   + Trigger (manual, CPU 单元) — GPU/CPU 混合图
//! - t3 超越函数图: Sin→Cos 链 + Log (GPU 单元, ≤1e-5 相对容差)
//!
//! 断言: DataBus 样本序列 (精确算子位级一致) / 派生环内容与点数 /
//! output_snapshot (精确节点位级) / source_frames 缓存 一致;
//! 5000 帧跨块 (EVAL_CHUNK) 分块推进; GPU 路径连跑两次验证确定性。
//!
//! 无 GPU 适配器环境 (部分 CI) 整组跳过。

use std::collections::HashMap;

use app_state::AppState;
use dsp_filter::FilterConfig;
use node_engine::CompiledGraph;
use node_kind::{MathOp, StrOp};
use node_testkit::{
    edge, make_filter, make_input, make_math, make_protocol_source, make_sink, make_str,
    make_text_input, make_trigger, trigger_rule,
};
use node_trigger::TriggerMatchType;
use pipeline_bus::TopicKey;
use pipeline_data_plane::frame_dispatch;
use vofa_core::{DataFrame, EvalBackend, PipelineConfig};

/// t1 — GPU/CPU 混合图: Math 链 (GPU 单元) + Filter+Trigger 链 (CPU 单元)
/// + Str 链 (CPU 单元)。注: trig 挂 m3 (m2 链保持纯 Math 可上 GPU)。
fn mixed_graph() -> CompiledGraph {
    let nodes = vec![
        make_protocol_source("ps1", "t1", "pt", 4),
        make_input("knob1", "t1"),
        // GPU 单元: ps.ch0 × ps.ch1 + knob1
        make_math("m1", "t1", MathOp::Mul, 2),
        make_math("m2", "t1", MathOp::Add, 2),
        // CPU 单元: 低通 (状态) + ps.ch3
        make_filter(
            "f1",
            "t1",
            FilterConfig::Lowpass {
                cutoff: 100.0,
                sample_rate: 1000.0,
            },
        ),
        make_math("m3", "t1", MathOp::Add, 2),
        // CPU 单元: 字符串平面 → 数值 + Trigger (manual)
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

/// t3 — 超越函数图: Sin→Cos 链 + Log (ps.ch1 > 0 保证定义域)
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

fn install(state: &AppState) {
    let mut graphs = state.data_plane.eval.graphs.lock();
    graphs.insert("t1".into(), mixed_graph());
    graphs.insert("t3".into(), transcendent_graph());
    drop(graphs);
    state
        .data_plane
        .eval
        .input_values
        .write()
        .insert("knob1".into(), 2.0);
}

fn set_config(state: &AppState, backend: EvalBackend) {
    *state.data_plane.pipeline_config.write() = PipelineConfig {
        eval_backend: backend,
        ..PipelineConfig::default()
    };
}

type BusRx = tokio::sync::broadcast::Receiver<std::sync::Arc<pipeline_bus::SampleBatch>>;

const TOPICS: [(&str, &str); 6] = [
    ("m2", "result"),
    ("m3", "result"),
    ("m4", "result"),
    ("trig", "value"),
    ("t_cos", "result"),
    ("t_log", "result"),
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

struct Observed {
    bus: HashMap<String, Vec<f64>>,
    snapshot: node_engine::ValuesMap,
    derived: HashMap<String, Vec<f32>>,
    source_frame: Option<Vec<f32>>,
}

async fn observe(state: &AppState, rx: &mut HashMap<String, BusRx>) -> Observed {
    let bus = drain_bus(rx).await;
    let buf = state.data_plane.buffer_for("pt");
    let mut b = buf.lock();
    let mut derived = HashMap::new();
    for (sink, source) in [("sinkA", "m2"), ("sinkT1", "t_cos"), ("sinkT2", "t_log")] {
        let idx = b.derived_index_of(sink, source);
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

/// 5000 帧批次 — 跨 2 块 (EVAL_CHUNK = 4096)
fn big_batch() -> Vec<DataFrame> {
    (0..5000)
        .map(|i| {
            let f = f32::from(u16::try_from(i % 256).unwrap_or(0)) - 128.0;
            DataFrame::with_timestamp(
                u64::try_from(i).unwrap_or(0) * 100,
                vec![f, f.abs() + 0.5, f * 0.5, f + 3.0],
            )
        })
        .collect()
}

async fn run_fixture(backend: EvalBackend) -> Observed {
    let state = AppState::new();
    install(&state);
    set_config(&state, backend);
    let mut rx = subscribe_all(&state).await;
    frame_dispatch::on_frames(&state.data_plane, "pt", &big_batch());
    observe(&state, &mut rx).await
}

/// 对称相对误差 (÷/超越函数 ≤2.5 ulp ≪ 1e-5)
fn close_enough(a: f64, b: f64) -> bool {
    (a - b).abs() / (a.abs() + b.abs()).max(1e-12) < 1e-5
}

#[tokio::test]
async fn gpu_matches_serial_full_pipeline() {
    let Some(_ctx) = gpu_core::GpuContext::acquire() else {
        eprintln!("无 GPU 适配器, 跳过 GPU 等价测试");
        return;
    };
    let serial = run_fixture(EvalBackend::Cpu).await;
    let gpu = run_fixture(EvalBackend::Gpu).await;

    // 精确算子 (GPU Math 链 + CPU Filter/Trigger/Str) — 位级一致
    for topic in ["m2/result", "m3/result", "m4/result", "trig/value"] {
        let (sv, gv) = (serial.bus.get(topic).unwrap(), gpu.bus.get(topic).unwrap());
        eprintln!("LEN {topic}: serial={} gpu={}", sv.len(), gv.len());
        for (i, (a, b)) in sv.iter().zip(gv.iter()).enumerate() {
            if a.to_bits() != b.to_bits() {
                eprintln!("DIFF {topic}[{i}]: serial={a} gpu={b}");
                if i > 6 { break; }
            }
        }
        assert_eq!(
            serial.bus.get(topic).unwrap(),
            gpu.bus.get(topic).unwrap(),
            "{topic} 值序列应位级一致"
        );
    }
    // 超越函数 — 相对容差
    for topic in ["t_cos/result", "t_log/result"] {
        let (s, g) = (
            serial.bus.get(topic).unwrap(),
            gpu.bus.get(topic).unwrap(),
        );
        assert_eq!(s.len(), g.len(), "{topic} 样本数一致");
        for (a, b) in s.iter().zip(g.iter()) {
            assert!(
                close_enough(*a, *b),
                "{topic}: gpu {b} vs cpu {a} 超出容差"
            );
        }
    }

    // 派生环: 精确算子位级 / 超越函数容差; 点数一致
    for key in ["sinkA/m2", "sinkT1/t_cos", "sinkT2/t_log"] {
        let (s, g) = (
            serial.derived.get(key).unwrap(),
            gpu.derived.get(key).unwrap(),
        );
        assert_eq!(s.len(), g.len(), "派生环 {key} 点数一致");
        let tolerance = key.contains("t_cos") || key.contains("t_log");
        for (a, b) in s.iter().zip(g.iter()) {
            if tolerance {
                assert!(
                    close_enough(f64::from(*a), f64::from(*b)),
                    "派生环 {key}: gpu {b} vs cpu {a}"
                );
            } else {
                assert_eq!(a.to_bits(), b.to_bits(), "派生环 {key} 应位级一致");
            }
        }
    }

    // 快照: 精确节点位级 (m2); 超越函数容差 (t_cos/t_log)
    let get = |o: &Observed, node: &str| {
        o.snapshot
            .get(node)
            .and_then(|m| m.get("result"))
            .copied()
    };
    assert_eq!(get(&serial, "m2"), get(&gpu, "m2"), "快照 m2 位级一致");
    for node in ["t_cos", "t_log"] {
        let (a, b) = (
            get(&serial, node).expect("cpu 快照有值"),
            get(&gpu, node).expect("gpu 快照有值"),
        );
        assert!(close_enough(f64::from(a), f64::from(b)), "快照 {node} 容差");
    }

    // source_frames 批尾缓存一致
    assert_eq!(serial.source_frame, gpu.source_frame);
}

#[tokio::test]
async fn gpu_path_is_deterministic() {
    let Some(_ctx) = gpu_core::GpuContext::acquire() else {
        eprintln!("无 GPU 适配器, 跳过 GPU 确定性测试");
        return;
    };
    let run1 = run_fixture(EvalBackend::Gpu).await;
    let run2 = run_fixture(EvalBackend::Gpu).await;
    for topic in TOPICS.map(|(n, p)| format!("{n}/{p}")) {
        assert_eq!(
            run1.bus.get(&topic).unwrap(),
            run2.bus.get(&topic).unwrap(),
            "{topic} GPU 路径确定性"
        );
    }
    assert_eq!(run1.derived, run2.derived, "派生环确定性");
}
