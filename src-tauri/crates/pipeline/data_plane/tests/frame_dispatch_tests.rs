//! frame_dispatch 模块集成测试
//!
//! Protocol 节点产帧 → source_frames 缓存 + 数值平面触发的端到端验证。

use app_state::AppState;
use data_plane::frame_dispatch;
use engine::CompiledGraph;
use kind::{NodeDef, NodeKind, StrNumParams};
use vofa_core::DataFrame;

#[test]
fn evaluation_waiting_on_inputs_does_not_lock_raw_recording() {
    use std::time::{Duration, Instant};
    let plane = AppState::new().data_plane;
    let input_guard = plane.eval.input_values.write();
    let worker_plane = plane.clone();
    let worker = std::thread::spawn(move || {
        let buffer = worker_plane.buffer_for("pt");
        frame_dispatch::eval_frames(
            &worker_plane.eval,
            &worker_plane.global_nodes,
            &buffer,
            "pt",
            &[DataFrame::with_timestamp(1, vec![1.0])],
            frame_dispatch::EvalOptions {
                workers: 1,
                simd: false,
            },
        );
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut reached_eval = false;
    while Instant::now() < deadline {
        if plane.source_frames.try_lock().is_none() {
            reached_eval = true;
            break;
        }
        std::thread::yield_now();
    }
    let buffer = plane.buffer_for("pt");
    let raw_available = buffer.try_lock().is_some();
    drop(input_guard);
    worker.join().unwrap();
    assert!(reached_eval, "评估线程应已拿到来源锁并等待输入锁");
    assert!(raw_available, "慢评估不得持有原始记录锁");
}

#[test]
fn history_is_only_recorded_for_computed_waveform_inputs() {
    use testkit::{edge, make_math, make_protocol_source, make_sink};
    for workers in [1, 4] {
        let plane = AppState::new().data_plane;
        plane.pipeline_config.write().eval_workers = workers;
        let graph = CompiledGraph::compile(
            "t".into(),
            vec![
                make_protocol_source("ps", "t", "pt", 1),
                make_math("math", "t", kind::MathOp::Add, 1),
                make_sink("wave", "t"),
                make_sink("raw", "t"),
                make_sink("gauge", "t"),
            ],
            vec![
                edge("input", "ps", "ch0", "math", "in0"),
                edge("history", "math", "result", "wave", "CH0"),
                edge("raw", "ps", "ch0", "raw", "CH0"),
                edge("latest", "math", "result", "gauge", "value"),
            ],
        )
        .unwrap();
        plane.eval.graphs.lock().insert("t".into(), graph);
        let frames = [
            DataFrame::with_timestamp(1_000, vec![2.0]),
            DataFrame::with_timestamp(2_000, vec![3.0]),
        ];
        frame_dispatch::on_frames(&plane, "pt", &frames);
        let window = plane.buffer_for("pt").lock().get_recent(2);
        assert_eq!(window.derived.len(), 1);
        assert_eq!(window.derived["wave"]["math"]["result"], vec![2.0, 3.0]);
        assert_eq!(window.channels[0], vec![2.0, 3.0]);
    }
}

/// 数值平面端到端: ProtocolSource 引用 pt 源, on_frames 后快照/缓冲应有值
#[test]
fn on_frames_triggers_numeric_plane() {
    let state = AppState::new();
    let plane = state.data_plane;
    let graph = CompiledGraph::compile(
        "t1".into(),
        vec![NodeDef {
            id: "src1".into(),
            tab_id: "t1".into(),
            kind: NodeKind::ProtocolSource {
                node_id: "pt".into(),
                channels: 2,
                port_names: None,
            },
        }],
        vec![],
    )
    .unwrap();
    plane.eval.graphs.lock().insert("t1".into(), graph);

    let frames = vec![
        DataFrame::with_timestamp(1000, vec![1.0, 2.0]),
        DataFrame::with_timestamp(2000, vec![3.0, 4.0]),
    ];
    let ns = frame_dispatch::on_frames(&plane, "pt", &frames);
    assert!(ns > 0 || frames.len() == 2); // 耗时仅观测, 不断言

    // source_frames 缓存为最新帧
    assert_eq!(
        plane.source_frames.lock().get("pt").unwrap().channels,
        vec![3.0, 4.0]
    );
    // 按源 DataBuffer 收到 2 帧
    let buf = plane.buffer_for("pt");
    let b = buf.lock();
    assert_eq!(b.point_count(), 2);
    assert_eq!(b.get_channel(0, 2), vec![1.0, 3.0]);
    drop(b);
    // 快照含 ProtocolSource 输出 (批尾发布)
    let snap = plane.eval.output_snapshot.lock();
    let ports = snap.values.get("src1").expect("src1 应有输出");
    assert_eq!(ports.get("ch0"), Some(&3.0));
    assert_eq!(ports.get("ch1"), Some(&4.0));
}

/// 不引用该源的图不被触发 (含其他 ProtocolSource 的图)
#[test]
fn on_frames_skips_unrelated_graphs() {
    let state = AppState::new();
    let plane = state.data_plane;
    let graph = CompiledGraph::compile(
        "t1".into(),
        vec![NodeDef {
            id: "src_other".into(),
            tab_id: "t1".into(),
            kind: NodeKind::ProtocolSource {
                node_id: "other".into(),
                channels: 1,
                port_names: None,
            },
        }],
        vec![],
    )
    .unwrap();
    plane.eval.graphs.lock().insert("t1".into(), graph);

    frame_dispatch::on_frames(&plane, "pt", &[DataFrame::with_timestamp(1, vec![9.0])]);
    // 图引用的是 "other" 源, 不被 "pt" 触发 → 快照无 src_other 输出
    let snap = plane.eval.output_snapshot.lock();
    assert!(!snap.values.contains_key("src_other"));
}

/// RawData 文本缓存 → ProtocolSource "str" 端口 (值平面字符串通路端到端)
#[test]
fn cache_source_text_feeds_str_port() {
    use kind::StrOp;
    let state = AppState::new();
    let plane = state.data_plane;
    let graph = CompiledGraph::compile(
        "t1".into(),
        vec![
            NodeDef {
                id: "src1".into(),
                tab_id: "t1".into(),
                kind: NodeKind::ProtocolSource {
                    node_id: "rd".into(),
                    channels: 1,
                    port_names: Some(vec!["str".to_string()]),
                },
            },
            NodeDef {
                id: "mid1".into(),
                tab_id: "t1".into(),
                kind: NodeKind::Str {
                    op: StrOp::Upper,
                    num: StrNumParams::default(),
                    tmpl: String::new(),
                },
            },
        ],
        vec![buffer_graph::Edge {
            id: "e1".into(),
            source: "src1".into(),
            source_handle: "str".into(),
            target: "mid1".into(),
            target_handle: "str".into(),
        }],
    )
    .unwrap();
    plane.eval.graphs.lock().insert("t1".into(), graph);

    // 多字节字符 (证明按字符而非字节的安全解码) + latest-value 覆盖
    frame_dispatch::cache_source_text(&plane, "rd", "你好 world".as_bytes());
    frame_dispatch::refresh_snapshot(&plane);
    let out = plane.eval.graph_string_outputs.lock();
    // 直通端口: src1 的 str 口 = 缓存的原始文本
    assert_eq!(
        out.get("src1").and_then(|p| p.get("str")),
        Some(&"你好 world".to_string()),
        "RawData 文本应经 ProtocolSource 的 str 端口进入字符串平面"
    );
    // 下游求值: Str(Upper) 消费该文本后的输出
    assert_eq!(
        out.get("mid1").and_then(|p| p.get("result")),
        Some(&"你好 WORLD".to_string()),
        "下游 Str 节点应消费 RawData 文本参与求值"
    );
}

/// 时间戳由字节平面采样时钟权威给定, 数值平面原样入库不做任何加工:
/// 逐帧递增时间戳保持递增 (相对最新毫秒换算正确)
#[test]
fn on_frames_preserves_authoritative_timestamps() {
    let state = AppState::new();
    let plane = state.data_plane;

    let frames = vec![
        DataFrame::with_timestamp(1_000, vec![1.0]),
        DataFrame::with_timestamp(2_000, vec![2.0]),
        DataFrame::with_timestamp(3_000, vec![3.0]),
    ];
    frame_dispatch::on_frames(&plane, "pt", &frames);

    let buf = plane.buffer_for("pt");
    let b = buf.lock();
    let window = b.get_recent(3);
    assert_eq!(window.timestamps, vec![-2.0, -1.0, 0.0]);
}

/// 批内共享时间戳 (来自时钟域的合法输入) 原样保留 — 不再线性摊开:
/// 高于 µs 分辨率的采样率下相邻帧时间戳允许重合, min-max 显示降采样
/// 与窗口二分查询对非严格递增时间戳均正确
#[test]
fn on_frames_preserves_shared_batch_timestamps_verbatim() {
    let state = AppState::new();
    let plane = state.data_plane;

    // 100 帧共享同一时间戳: 数值平面不得用到达区间重写时间轴
    let batch: Vec<DataFrame> = (0..100_u16)
        .map(|i| DataFrame::with_timestamp(1_010_000, vec![f32::from(i)]))
        .collect();
    frame_dispatch::on_frames(&plane, "pt", &batch);

    let buf = plane.buffer_for("pt");
    let b = buf.lock();
    let window = b.get_recent(100);
    assert_eq!(window.timestamps.len(), 100);
    assert!(
        window.timestamps.iter().all(|ts| ts.abs() < f64::EPSILON),
        "共享时间戳应原样保留 (全部相对 0ms): {:?}",
        &window.timestamps[..4]
    );
    // 数值顺序不变
    assert_eq!(b.get_channel(0, 3), vec![97.0, 98.0, 99.0]);
    // 时间戳字节序与数值顺序一致 (同值)
    assert_eq!(b.time_bounds_us(), Some((1_010_000, 1_010_000)));
}

/// 非 UTF-8 字节 lossy 解码为 U+FFFD 替换符; 空数据覆盖写空文本 (latest-value 语义)
#[test]
fn cache_source_text_lossy_and_overwrite() {
    let state = AppState::new();
    let plane = state.data_plane;

    frame_dispatch::cache_source_text(&plane, "rd", &[0x68, 0xFF, 0x69]);
    assert_eq!(
        plane.source_texts.lock().get("rd").map(String::as_str),
        Some("h\u{FFFD}i"),
        "非法 UTF-8 字节应替换为 U+FFFD 而非报错"
    );

    frame_dispatch::cache_source_text(&plane, "rd", b"");
    assert_eq!(
        plane.source_texts.lock().get("rd").map(String::as_str),
        Some(""),
        "空批次按空文本覆盖 (保持既有 latest-value 行为)"
    );

    // 多源隔离: 其他源缓存不受影响
    frame_dispatch::cache_source_text(&plane, "rd2", b"other");
    assert_eq!(
        plane.source_texts.lock().get("rd").map(String::as_str),
        Some("")
    );
}
