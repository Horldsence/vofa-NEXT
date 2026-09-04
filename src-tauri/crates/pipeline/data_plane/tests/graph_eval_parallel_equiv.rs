//! 并发/串行评估等价性回归 — chunked fork-join (eval_workers ≥ 2) 与
//! 串行热路径 (eval_workers = 1) 的可观测输出逐字节一致。
//!
//! 覆盖图集 (动态图 t1 + 静态图 t2):
//! - 独立 Math 链 ×2 (跨单元扇出的供给节点 ProtocolSource/Input)
//! - Filter 状态链 (状态按单元切分 + 跨帧演化)
//! - Trigger (manual, 状态 record_prev) + Sink 派生边
//! - Str LEN (字符串平面 → 数值交叉)
//! - 静态纯本地图 (每批评估一次; DataBus 密度 delta 单独断言)
//!
//! 断言: DataBus 样本序列 / output_snapshot / 派生环内容 / graph_string_outputs /
//! source_frames 缓存 全部一致; 并行连跑两次验证确定性。

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

/// 动态图 — 多路径 + 有状态节点 (t1)
fn dynamic_graph() -> CompiledGraph {
    let nodes = vec![
        make_protocol_source("ps1", "t1", "pt", 4),
        make_input("knob1", "t1"),
        // 路径 A: ps.ch0 × ps.ch1 + knob1
        make_math("m1", "t1", MathOp::Mul, 2),
        make_math("m2", "t1", MathOp::Add, 2),
        // 路径 B: 低通 (状态) + ps.ch3
        make_filter(
            "f1",
            "t1",
            FilterConfig::Lowpass {
                cutoff: 100.0,
                sample_rate: 1000.0,
            },
        ),
        make_math("m3", "t1", MathOp::Add, 2),
        // 路径 C: 字符串平面 → 数值
        make_text_input("textin", "t1", "abcd"),
        make_str("slen", "t1", StrOp::Len),
        make_math("m4", "t1", MathOp::Add, 2),
        // Trigger (manual, 状态 record_prev)
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
        // Sink (派生边回写验证)
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
        edge("e11", "m2", "result", "trig", "trigger"),
        edge("e12", "m2", "result", "sinkA", "CH0"),
        edge("e13", "m3", "result", "sinkB", "CH0"),
        edge("e14", "m4", "result", "sinkC", "CH0"),
        edge("e15", "trig", "value", "sinkD", "CH0"),
    ];
    CompiledGraph::compile("t1".into(), nodes, edges).unwrap()
}

/// 静态纯本地图 — Input+Input → Mul (t2, 输出批内常量)
fn static_graph() -> CompiledGraph {
    let nodes = vec![
        make_input("knobA", "t2"),
        make_input("knobB", "t2"),
        make_math("mS", "t2", MathOp::Mul, 2),
        make_sink("sinkS", "t2"),
    ];
    let edges = vec![
        edge("s1", "knobA", "value", "mS", "in0"),
        edge("s2", "knobB", "value", "mS", "in1"),
        edge("s3", "mS", "result", "sinkS", "CH0"),
    ];
    CompiledGraph::compile("t2".into(), nodes, edges).unwrap()
}

fn install(state: &AppState) {
    let mut graphs = state.data_plane.eval.graphs.lock();
    graphs.insert("t1".into(), dynamic_graph());
    graphs.insert("t2".into(), static_graph());
    drop(graphs);
    let mut inputs = state.data_plane.eval.input_values.write();
    inputs.insert("knob1".into(), 2.0);
    inputs.insert("knobA".into(), 5.0);
    inputs.insert("knobB".into(), 7.0);
}

fn set_workers(state: &AppState, n: usize) {
    *state.data_plane.pipeline_config.write() = PipelineConfig {
        eval_workers: n,
        ..PipelineConfig::default()
    };
}

const TOPICS: [(&str, &str); 6] = [
    ("m2", "result"),
    ("f1", "result"),
    ("m3", "result"),
    ("m4", "result"),
    ("trig", "value"),
    ("mS", "result"), // 静态图 (密度 delta 单独断言)
];

fn batch1() -> Vec<DataFrame> {
    [
        (0.0, 1_000),
        (1.0, 2_000),
        (2.0, 3_000),
        (3.0, 4_000),
        (4.0, 5_000),
    ]
    .into_iter()
    .map(|(i, ts)| DataFrame::with_timestamp(ts, vec![i, i * 2.0, i * 0.5, i + 3.0]))
    .collect()
}

fn batch2() -> Vec<DataFrame> {
    [(0.0, 10_000), (1.0, 11_000), (2.0, 12_000), (3.0, 13_000)]
        .into_iter()
        .map(|(i, ts)| DataFrame::with_timestamp(ts, vec![10.0 - i, 3.0, 8.0 - i, 1.5]))
        .collect()
}

/// 采集当前全部可观测状态 (bus 在调用前已订阅, 由调用方 drain)
struct Observed {
    bus: HashMap<String, Vec<f64>>,
    snapshot: engine::ValuesMap,
    strings: HashMap<String, HashMap<String, String>>,
    derived: HashMap<String, Vec<f32>>,
    derived_points: usize,
    source_frame: Option<Vec<f32>>,
}

type BusRx = tokio::sync::broadcast::Receiver<std::sync::Arc<data_bus::SampleBatch>>;

async fn subscribe_all(state: &AppState) -> HashMap<String, BusRx> {
    let mut rx = HashMap::new();
    for (node, port) in TOPICS {
        let key = TopicKey::new(node, port);
        let receiver = state
            .data_plane
            .eval
            .data_bus
            .subscribe(key, 4096)
            .await
            .expect("订阅主题应存在");
        rx.insert(format!("{node}/{port}"), receiver);
    }
    rx
}

async fn observe(state: &AppState, rx: &mut HashMap<String, BusRx>) -> Observed {
    // publish 经 topic actor 异步转发 — 轮询让路, 直到静默窗口无新批次
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
    let buf = state.data_plane.buffer_for("pt");
    let b = buf.lock();
    let mut derived = HashMap::new();
    let mut derived_points = 0;
    for (sink, source, handle) in [
        ("sinkA", "m2", "result"),
        ("sinkB", "m3", "result"),
        ("sinkC", "m4", "result"),
        ("sinkD", "trig", "value"),
    ] {
        let idx = b.derived_port_index_of(sink, source, handle);
        let recent = b.get_derived(idx, 10_000);
        derived_points = recent.len();
        derived.insert(format!("{sink}/{source}"), recent);
    }
    let points = b.point_count();
    drop(b);
    let _ = points;

    Observed {
        bus,
        snapshot: state.data_plane.eval.output_snapshot.lock().values.clone(),
        strings: state.data_plane.eval.graph_string_outputs.lock().clone(),
        derived,
        derived_points,
        source_frame: state
            .data_plane
            .eval
            .source_frames
            .lock()
            .get("pt")
            .map(|f| f.channels.clone()),
    }
}

/// 跑完整批次序列并采集
async fn run_fixture(workers: usize) -> (Observed, Observed) {
    let state = AppState::new();
    install(&state);
    set_workers(&state, workers);
    let mut rx = subscribe_all(&state).await;

    frame_dispatch::on_frames(&state.data_plane, "pt", &batch1());
    let obs1 = observe(&state, &mut rx).await;
    frame_dispatch::on_frames(&state.data_plane, "pt", &batch2());
    let obs2 = observe(&state, &mut rx).await;
    (obs1, obs2)
}

#[tokio::test]
async fn parallel_matches_serial_full_pipeline() {
    let (serial1, serial2) = run_fixture(1).await;
    let (par1, par2) = run_fixture(4).await;

    // 动态图: 逐帧值序列一致 (mS 静态图单测覆盖)
    for topic in [
        "m2/result",
        "f1/result",
        "m3/result",
        "m4/result",
        "trig/value",
    ] {
        assert_eq!(
            serial1.bus.get(topic).unwrap(),
            par1.bus.get(topic).unwrap(),
            "批1 {topic} 值序列不一致"
        );
        assert_eq!(
            serial2.bus.get(topic).unwrap(),
            par2.bus.get(topic).unwrap(),
            "批2 {topic} 值序列不一致"
        );
    }

    // 派生环: 内容与点数一致
    for key in ["sinkA/m2", "sinkB/m3", "sinkC/m4", "sinkD/trig"] {
        assert_eq!(
            serial1.derived.get(key).unwrap(),
            par1.derived.get(key).unwrap(),
            "批1 派生环 {key} 不一致"
        );
        assert_eq!(
            serial2.derived.get(key).unwrap(),
            par2.derived.get(key).unwrap(),
            "批2 派生环 {key} 不一致"
        );
        assert_eq!(serial1.derived_points, par1.derived_points);
    }

    // 快照 (latest-value) 一致
    assert_eq!(serial1.snapshot, par1.snapshot, "批1 快照不一致");
    assert_eq!(serial2.snapshot, par2.snapshot, "批2 快照不一致");

    // 字符串输出一致 (slen/trigger.text 等)
    assert_eq!(serial1.strings, par1.strings);
    assert_eq!(serial2.strings, par2.strings);

    // source_frames 批尾缓存一致
    assert_eq!(serial1.source_frame, par1.source_frame);
    assert_eq!(serial2.source_frame, par2.source_frame);

    // 静态图: 两条路径一致 — 每批批尾单样本 (批内输入不变, 逐帧重复发布是纯浪费)
    let serial_static = serial2.bus.get("mS/result").unwrap();
    let par_static = par2.bus.get("mS/result").unwrap();
    assert_eq!(serial_static.len(), 1, "串行静态图应每批发布一次");
    assert_eq!(serial_static, par_static, "静态图两路径发布一致");
    assert_eq!(
        serial1.bus.get("mS/result").unwrap().len(),
        1,
        "批1 同样单样本"
    );

    // 确定性: 并行连跑两次完全一致
    let (par1b, par2b) = run_fixture(4).await;
    for topic in TOPICS.map(|(n, p)| format!("{n}/{p}")) {
        assert_eq!(
            par1.bus.get(&topic),
            par1b.bus.get(&topic),
            "{topic} 确定性"
        );
        assert_eq!(
            par2.bus.get(&topic),
            par2b.bus.get(&topic),
            "{topic} 确定性"
        );
    }
}

#[tokio::test]
async fn parallel_matches_serial_across_chunks() {
    // 3000 帧 = 3 块 (EVAL_CHUNK=1024) — 锁定块间 staging 交换/回放不重复不丢失
    let frames: Vec<DataFrame> = (0..3000)
        .enumerate()
        .map(|(i, v)| {
            let f = f32::from(u16::try_from(v % 64).unwrap_or(0));
            DataFrame::with_timestamp(
                u64::try_from(i).unwrap_or(0) * 100,
                vec![f, f * 2.0, f * 0.5, f + 1.0],
            )
        })
        .collect();

    let run = |workers: usize| {
        let state = AppState::new();
        install(&state);
        set_workers(&state, workers);
        frame_dispatch::on_frames(&state.data_plane, "pt", &frames);
        let buf = state.data_plane.buffer_for("pt");
        let b = buf.lock();
        let mut derived = Vec::new();
        for (sink, source, handle) in [
            ("sinkA", "m2", "result"),
            ("sinkB", "m3", "result"),
            ("sinkC", "m4", "result"),
        ] {
            let idx = b.derived_port_index_of(sink, source, handle);
            derived.push(b.get_derived(idx, 10_000));
        }
        (b.point_count(), derived, state)
    };

    let (serial_points, serial_derived, serial_state) = run(1);
    let (par_points, par_derived, par_state) = run(4);

    assert_eq!(serial_points, par_points, "点数一致");
    assert_eq!(
        serial_derived, par_derived,
        "跨块派生环内容不一致 (重复回放/丢失)"
    );
    assert_eq!(
        serial_state.data_plane.eval.output_snapshot.lock().values,
        par_state.data_plane.eval.output_snapshot.lock().values
    );
}

#[tokio::test]
async fn other_source_triggers_static_only() {
    let state = AppState::new();
    install(&state);
    set_workers(&state, 4);
    let mut rx = subscribe_all(&state).await;

    // 先喂 pt 建立动态图输出
    frame_dispatch::on_frames(&state.data_plane, "pt", &batch1());
    let obs1 = observe(&state, &mut rx).await;
    let dynamic_samples: usize = ["m2/result", "f1/result", "m3/result", "m4/result"]
        .iter()
        .map(|t| obs1.bus.get(*t).unwrap().len())
        .sum();
    assert!(dynamic_samples > 0);

    // pt2 批: 动态图不触发 (引用 pt), 静态图触发 → 仅静态图 1 个新样本
    let pt2_frames = vec![DataFrame::with_timestamp(20_000, vec![9.0; 2])];
    frame_dispatch::on_frames(&state.data_plane, "pt2", &pt2_frames);
    let obs2 = observe(&state, &mut rx).await;

    for t in [
        "m2/result",
        "f1/result",
        "m3/result",
        "m4/result",
        "trig/value",
    ] {
        assert!(
            obs2.bus.get(t).unwrap().is_empty(),
            "pt2 批不应发布动态图样本: {t}"
        );
    }
    assert_eq!(obs2.bus.get("mS/result").unwrap(), &vec![35.0]);
    // 静态图快照保持 (latest-value 融合)
    assert_eq!(
        obs2.snapshot.get("mS").and_then(|m| m.get("result")),
        Some(&35.0)
    );
}
