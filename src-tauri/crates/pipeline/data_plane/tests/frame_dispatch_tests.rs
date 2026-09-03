//! frame_dispatch 模块集成测试
//!
//! Protocol 节点产帧 → source_frames 缓存 + 数值平面触发的端到端验证。

use app_state::AppState;
use data_plane::frame_dispatch;
use engine::CompiledGraph;
use kind::{NodeDef, NodeKind, StrNumParams};
use vofa_core::DataFrame;

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
