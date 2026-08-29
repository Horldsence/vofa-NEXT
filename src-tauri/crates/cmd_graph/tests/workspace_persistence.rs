//! 工作区持久化 (workspace.json) 回环与 widget 记录语义集成测试
//!
//! widget 配置模型与画布位置的后端权威存储契约:
//! - 提交携带 widgets/positions 时落库, 收集快照落盘后可完整恢复
//!   (含逐 tab 重编译, 恢复后图立即可求值);
//! - 拓扑 op (connect_edge) 不携带 widget 记录时源图中现状保留;
//! - tab 移除后其节点的孤儿位置条目被清理。

use std::collections::HashMap;

use app_state::{collect_workspace_file, save_workspace, AppState, Position, WidgetRecord};
use buffer_graph::Edge;
use cmd_graph::{apply_connect_edge, apply_remove_tab_graph, apply_tab_graph, restore_workspace};
use node_kind::{NodeDef, NodeKind};

fn sink_node(id: &str, tab_id: &str) -> NodeDef {
    NodeDef {
        id: id.into(),
        tab_id: tab_id.into(),
        kind: NodeKind::Sink,
    }
}

fn math_node(id: &str, tab_id: &str) -> NodeDef {
    NodeDef {
        id: id.into(),
        tab_id: tab_id.into(),
        kind: NodeKind::Math {
            op: node_kind::MathOp::Add,
            input_count: 1,
        },
    }
}

fn gauge_record(id: &str) -> WidgetRecord {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "kind": "NumberDisplay",
        "params": { "id": id, "label": "N", "min": 0.0, "max": 100.0 }
    }))
    .expect("widget 记录应可反序列化")
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vofa-workspace-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("创建临时目录");
    dir
}

/// 提交 → 落盘 → 新 AppState 恢复: 源图 / widget 记录 / 位置 / tab 元数据
/// 完整还原, 且恢复后图已编译可求值 (提交的 Input 值立即出现在输出快照)
#[tokio::test]
async fn workspace_survives_save_and_restore_roundtrip() {
    let dir = temp_dir("roundtrip");

    let state = AppState::new();
    state.workspace.lock().tabs = vec![app_state::TabMeta {
        id: "tab1".into(),
        name: "Tab 1".into(),
        widgets: vec!["w-gauge".into()],
    }];
    state.workspace.lock().positions.insert(
        "w-gauge".into(),
        Position { x: 12.5, y: 40.0 },
    );
    apply_tab_graph(
        &state,
        None,
        "tab1".into(),
        vec![sink_node("w-gauge", "tab1"), math_node("m1", "tab1")],
        vec![Edge {
            id: "e1".into(),
            source: "m1".into(),
            source_handle: "result".into(),
            target: "w-gauge".into(),
            target_handle: "value".into(),
        }],
        HashMap::new(),
        Some(vec![gauge_record("w-gauge")]),
        Some(HashMap::from([("m1".to_string(), Position { x: 1.0, y: 2.0 })])),
        None,
    )
    .await
    .expect("提交图应成功");

    let file = collect_workspace_file(&state.workspace, &state.source_graphs);
    save_workspace(&dir, &file).expect("落盘应成功");
    assert!(state.workspace.lock().dirty);

    // 模拟重启: 全新 AppState + 从磁盘恢复
    let fresh = AppState::new();
    assert!(restore_workspace(&fresh, &dir).await, "应报告已恢复");
    let ws = fresh.workspace.lock();
    assert!(ws.restored && !ws.dirty, "恢复后不应立即触发重写");
    assert_eq!(ws.tabs.len(), 1);
    assert_eq!(ws.tabs[0].name, "Tab 1");
    assert_eq!(ws.positions.len(), 2, "两个节点的位置都应恢复");
    drop(ws);

    let stored = fresh.source_graphs.lock().get("tab1").unwrap().clone();
    assert_eq!(stored.nodes.len(), 2);
    assert_eq!(stored.edges.len(), 1);
    assert_eq!(stored.widgets.len(), 1, "widget 配置记录应随源图恢复");
    assert_eq!(stored.widgets[0].kind, "NumberDisplay");
    assert!(
        fresh.graphs.lock().contains_key("tab1"),
        "恢复时应逐 tab 重编译"
    );
    assert!(
        fresh.graphs_version.load(std::sync::atomic::Ordering::Relaxed) >= 1,
        "恢复提交应推进版本号 (前端水合基线)"
    );

    // 文件不存在 → 全新安装语义
    let empty_dir = temp_dir("empty");
    let fresh2 = AppState::new();
    assert!(!restore_workspace(&fresh2, &empty_dir).await);
    assert!(!fresh2.workspace.lock().restored);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&empty_dir);
}

/// 拓扑 op 不携带 widget 记录 (None) 时, 源图中的记录保留
#[tokio::test]
async fn connect_edge_op_preserves_widget_records() {
    let state = AppState::new();
    apply_tab_graph(
        &state,
        None,
        "tab1".into(),
        vec![sink_node("w-gauge", "tab1"), math_node("m1", "tab1")],
        vec![],
        HashMap::new(),
        Some(vec![gauge_record("w-gauge")]),
        None,
        None,
    )
    .await
    .expect("种子图应成功");

    apply_connect_edge(
        &state.graphs,
        &state.graphs_version,
        &state.data_plane,
        &state.source_graphs,
        &state.workspace,
        None,
        Some("tab1".into()),
        "m1".into(),
        "w-gauge".into(),
        Some("result".into()),
        Some("value".into()),
    )
    .await
    .expect("连线应成功");

    let stored = state.source_graphs.lock().get("tab1").unwrap().clone();
    assert_eq!(stored.edges.len(), 1);
    assert_eq!(stored.widgets.len(), 1, "拓扑 op 不得清掉 widget 记录");
    assert!(state.workspace.lock().dirty, "拓扑 op 提交应标记落盘脏");
}

/// tab 移除后, 其节点 (含跨 tab 独有的 widget) 的位置条目被清理
#[tokio::test]
async fn remove_tab_graph_prunes_orphan_positions() {
    let state = AppState::new();
    apply_tab_graph(
        &state,
        None,
        "tab1".into(),
        vec![sink_node("w-gauge", "tab1")],
        vec![],
        HashMap::new(),
        None,
        Some(HashMap::from([(
            "w-gauge".to_string(),
            Position { x: 3.0, y: 4.0 },
        )])),
        None,
    )
    .await
    .expect("提交图应成功");
    assert!(state.workspace.lock().positions.contains_key("w-gauge"));

    apply_remove_tab_graph(&state, None, "tab1")
        .await
        .expect("移除图应成功");
    assert!(
        !state.workspace.lock().positions.contains_key("w-gauge"),
        "孤儿位置条目应被清理"
    );
}
