//! graph_eval 字符串输出发布集成测试
//!
//! Str 节点求值结果 → graph_string_outputs → text_output_ticker 合并发布
//! 链路的发布接线与清理语义验证 (ticker 合并优先级单测见 app_state::tickers)。

use std::sync::atomic::Ordering;

use app_state::AppState;
use buffer_graph::Edge;
use node_engine::CompiledGraph;
use node_kind::{MathOp, NodeDef, NodeKind, StrNumParams, StrOp};
use pipeline_bus::TopicKey;
use pipeline_data_plane::{evaluate_snapshot_now, frame_dispatch, DataPlaneState};
use vofa_core::DataFrame;

/// 纯本地图 (无 ProtocolSource → 任意源来帧都触发), 含一个 Str 节点
fn str_node_graph(tab_id: &str) -> CompiledGraph {
    CompiledGraph::compile(
        tab_id.into(),
        vec![NodeDef {
            id: "up1".into(),
            tab_id: tab_id.into(),
            kind: NodeKind::Str {
                op: StrOp::Upper,
                num: StrNumParams::default(),
                tmpl: String::new(),
            },
        }],
        vec![],
    )
    .unwrap()
}

fn test_plane() -> (AppState, DataPlaneState) {
    let state = AppState::new();
    let plane = state.data_plane.clone();
    plane
        .eval
        .graphs
        .lock()
        .insert("t1".into(), str_node_graph("t1"));
    (state, plane)
}

#[tokio::test]
async fn event_driven_evaluation_publishes_active_numeric_topics() {
    let state = AppState::new();
    let plane = state.data_plane.clone();
    let graph = CompiledGraph::compile(
        "numeric-event".into(),
        vec![
            NodeDef {
                id: "input".into(),
                tab_id: "numeric-event".into(),
                kind: NodeKind::Input,
            },
            NodeDef {
                id: "math".into(),
                tab_id: "numeric-event".into(),
                kind: NodeKind::Math {
                    op: MathOp::Add,
                    input_count: 1,
                },
            },
        ],
        vec![Edge {
            id: "edge".into(),
            source: "input".into(),
            source_handle: "value".into(),
            target: "math".into(),
            target_handle: "in0".into(),
        }],
    )
    .unwrap();
    plane.eval.graphs.lock().insert("numeric-event".into(), graph);
    let mut receiver = plane
        .eval
        .data_bus
        .subscribe(TopicKey::new("math", "result"), 4)
        .await
        .unwrap();
    plane.eval.input_values.lock().insert("input".into(), 12.5);
    frame_dispatch::refresh_snapshot(&plane);
    let batch = receiver.recv().await.unwrap();
    assert_eq!(batch.samples.last().map(|sample| sample.value), Some(12.5));
}

/// 热路径: on_frames 批尾发布点应把 Str 节点字符串输出物化进 graph_string_outputs
#[test]
fn on_frames_publishes_str_outputs() {
    let (_state, plane) = test_plane();
    frame_dispatch::on_frames(&plane, "pt", &[DataFrame::with_timestamp(1, vec![1.0])]);

    let str_out = plane.eval.graph_string_outputs.lock();
    // Str 字符串域输出端口恒 written (无字符串源时输入缺省 "" → Upper("") = "")
    let ports = str_out.get("up1").expect("up1 应有字符串输出");
    assert_eq!(ports.get("result"), Some(&String::new()));
}

/// TextOut 桥接: 图内字符串 (TextInput → TextOut) 经通用发布进入 graph_string_outputs,
/// 且编译规格 (目标/换行后缀/间隔) 正确收集 — 发送 ticker 的数据契约端到端验证
#[test]
fn textout_publishes_to_string_outputs_with_specs() {
    let state = AppState::new();
    let plane = state.data_plane;
    let graph = CompiledGraph::compile(
        "t1".into(),
        vec![
            NodeDef {
                id: "textin".into(),
                tab_id: "t1".into(),
                kind: node_kind::NodeKind::TextInput {
                    text: "hello".into(),
                },
            },
            NodeDef {
                id: "tout".into(),
                tab_id: "t1".into(),
                kind: node_kind::NodeKind::TextOut {
                    target_transport: "tp1".into(),
                    newline: node_kind::NewlineMode::Lf,
                    min_interval_ms: 20,
                },
            },
        ],
        vec![buffer_graph::Edge {
            id: "e1".into(),
            source: "textin".into(),
            source_handle: "str".into(),
            target: "tout".into(),
            target_handle: "text".into(),
        }],
    )
    .unwrap();
    // 编译期规格收集
    {
        let specs = graph.compiled().textouts();
        assert_eq!(specs.len(), 1);
        assert_eq!(&*specs[0].node_id, "tout");
        assert_eq!(&*specs[0].target_transport, "tp1");
        assert_eq!(specs[0].newline_suffix, "\n");
        assert_eq!(specs[0].min_interval_ms, 20);
    }
    plane.eval.graphs.lock().insert("t1".into(), graph);

    frame_dispatch::on_frames(&plane, "pt", &[DataFrame::with_timestamp(1, vec![1.0])]);
    let str_out = plane.eval.graph_string_outputs.lock();
    assert_eq!(
        str_out.get("tout").and_then(|p| p.get("text")),
        Some(&"hello".to_string()),
        "TextOut 应把上游文本透传进通用字符串发布"
    );
}

/// 清理语义: 图重编译 (graphs_version 变化) 后批尾发布点清空过期节点条目 (同 f32 快照)
#[test]
fn version_change_clears_stale_str_outputs() {
    let (_state, plane) = test_plane();
    frame_dispatch::on_frames(&plane, "pt", &[DataFrame::with_timestamp(1, vec![1.0])]);
    assert!(plane.eval.graph_string_outputs.lock().contains_key("up1"));

    // 模拟图重编译: 注入过期节点条目 + 版本号 +1 (对齐 update_tab_graph/remove_tab_graph)
    plane.eval.graph_string_outputs.lock().insert(
        "stale_node".into(),
        [("result".into(), "old".into())].into(),
    );
    plane.eval.graphs_version.fetch_add(1, Ordering::Relaxed);

    frame_dispatch::on_frames(&plane, "pt", &[DataFrame::with_timestamp(2, vec![2.0])]);
    let str_out = plane.eval.graph_string_outputs.lock();
    assert!(
        !str_out.contains_key("stale_node"),
        "版本变化后过期节点条目应清空"
    );
    assert!(str_out.contains_key("up1"), "当前图节点应重新物化");
}

/// 慢路径: evaluate_snapshot_now 为全量覆盖写 — 过期节点条目随 swap 清理
#[test]
fn snapshot_eval_overwrites_str_outputs() {
    let (_state, plane) = test_plane();
    plane.eval.graph_string_outputs.lock().insert(
        "stale_node".into(),
        [("result".into(), "old".into())].into(),
    );

    let sf = plane.eval.source_frames.lock().clone();
    evaluate_snapshot_now(&plane.eval, &sf);

    let str_out = plane.eval.graph_string_outputs.lock();
    assert!(
        !str_out.contains_key("stale_node"),
        "快照评估全量覆盖写应清理过期条目"
    );
    assert_eq!(
        str_out.get("up1").and_then(|m| m.get("result")),
        Some(&String::new())
    );
}
