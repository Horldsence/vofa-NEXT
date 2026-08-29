//! remove_tab_graph 的全局节点归属语义集成测试
//!
//! 后端全局节点表 (DataPlaneState.global_nodes) 中每个节点带 tab_id = 最后
//! 提交它的 tab; remove_tab_graph 按 tab_id retain 清理。这些用例钉住该契约:
//! - 被删 tab 名下的全局节点 (Transport/Protocol) 随其图一并移除,
//!   对应 protocol_states 同步清除;
//! - 其他 tab 名下的全局节点不受影响。
//!
//! 该契约同时是前端 removeControlTab / applySnapshot 顺序要求的后端依据:
//! 先 sync 存活 tab (全局节点重新托管到存活 tab 名下), 再 remove 被删 tab。

use app_state::AppState;
use cmd_graph::{apply_remove_tab_graph, apply_tab_graph};
use node_kind::{NodeDef, NodeKind};
use schema_types::ProtocolConfig;
use vofa_core::config::{TestDataConfig, TransportConfig};

fn transport_node(id: &str, tab_id: &str) -> NodeDef {
    NodeDef {
        id: id.into(),
        tab_id: tab_id.into(),
        kind: NodeKind::Transport {
            config: TransportConfig::TestData(TestDataConfig::default()),
        },
    }
}

fn protocol_node(id: &str, tab_id: &str) -> NodeDef {
    NodeDef {
        id: id.into(),
        tab_id: tab_id.into(),
        kind: NodeKind::Protocol {
            config: ProtocolConfig::JustFloat { channels: Some(2) },
            convert_to: None,
            schema: None,
        },
    }
}

/// 被删 tab 名下的全局节点随图移除: Protocol 从全局表与 protocol_states 同时消失
/// (前端轮询的 "协议节点不存在" 即源于此 — 前端必须先重同步存活 tab)
#[tokio::test]
async fn remove_tab_graph_removes_nodes_owned_by_that_tab() {
    let state = AppState::new();
    apply_tab_graph(
        &state,
        None,
        "tab1".into(),
        vec![transport_node("tp", "tab1"), protocol_node("pt", "tab1")],
        vec![],
        Default::default(),
        None,
    )
    .await
    .expect("提交图应成功");
    assert!(
        state.data_plane.protocol_states.lock().contains_key("pt"),
        "前提: 提交后 protocol_states 含 pt"
    );

    apply_remove_tab_graph(&state, None, "tab1")
        .await
        .expect("移除图应成功");

    assert!(
        state.data_plane.global_nodes.lock().is_empty(),
        "被删 tab 名下的全局节点应一并移除"
    );
    assert!(
        state.data_plane.protocol_states.lock().is_empty(),
        "Protocol 运行时状态应随全局节点清除"
    );
}

/// 其他 tab 名下的全局节点存活: 存活 tab 重新提交同名节点后
/// (tab_id 已改为存活 tab), remove 被删 tab 不再误伤全局节点
#[tokio::test]
async fn remove_tab_graph_keeps_nodes_rehosted_to_surviving_tab() {
    let state = AppState::new();
    // tab1 最后提交全局节点 (归属 tab1)
    apply_tab_graph(
        &state,
        None,
        "tab1".into(),
        vec![transport_node("tp", "tab1"), protocol_node("pt", "tab1")],
        vec![],
        Default::default(),
        None,
    )
    .await
    .expect("提交 tab1 图应成功");
    // 存活 tab2 重同步: 全局节点按 id 覆盖, tab_id 改挂 tab2
    apply_tab_graph(
        &state,
        None,
        "tab2".into(),
        vec![transport_node("tp", "tab2"), protocol_node("pt", "tab2")],
        vec![],
        Default::default(),
        None,
    )
    .await
    .expect("提交 tab2 图应成功");

    apply_remove_tab_graph(&state, None, "tab1")
        .await
        .expect("移除图应成功");

    let nodes = state.data_plane.global_nodes.lock();
    assert!(
        nodes.contains_key("tp") && nodes.contains_key("pt"),
        "全局节点已重新托管到存活 tab, 不应被移除"
    );
    assert_eq!(nodes["pt"].tab_id, "tab2");
    drop(nodes);
    assert!(
        state.data_plane.protocol_states.lock().contains_key("pt"),
        "Protocol 运行时状态应保留"
    );
    assert!(
        state.graphs.lock().contains_key("tab2"),
        "存活 tab 的图不受影响"
    );
}
