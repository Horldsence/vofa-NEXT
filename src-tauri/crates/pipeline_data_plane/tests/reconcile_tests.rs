//! reconcile 模块集成测试
//!
//! 验证图重编译后孤儿资源清理的正确性:
//! - 节点从全局表消失 → 连接关闭 + raw 收集器清理
//! - 节点仍被其他 tab 引用 → 连接与收集器保留
//! - Protocol 节点删除 → protocol_states / buffers 清理

use app_state::AppState;
use node_engine::CompiledGraph;
use node_kind::{NodeDef, NodeKind};
use node_trigger::TriggerState;
use pipeline_data_plane::DataPlaneState;
use schema_types::{ProtocolConfig, TestDataLink};
use vofa_core::TransportConfig;

fn transport_node(id: &str, tab: &str) -> NodeDef {
    NodeDef {
        id: id.into(),
        tab_id: tab.into(),
        kind: NodeKind::Transport {
            config: TransportConfig::TestData(Default::default()),
        },
    }
}

fn protocol_node(id: &str, tab: &str) -> NodeDef {
    NodeDef {
        id: id.into(),
        tab_id: tab.into(),
        kind: NodeKind::Protocol {
            config: ProtocolConfig::default(),
            convert_to: None,
            schema: None,
        },
    }
}

async fn open_test_data(state: &AppState, id: &str, plane: &DataPlaneState) {
    state
        .transport
        .lock()
        .await
        .open(
            id,
            TransportConfig::TestData(Default::default()),
            TestDataLink::new(ProtocolConfig::RawData),
        )
        .await
        .unwrap();
    plane.raw_collector_for(id);
}

/// 节点从全局表消失 (所有 tab 不再引用) → 连接关闭 + raw 收集器清理
#[tokio::test]
async fn reconcile_closes_orphan_transport() {
    let state = AppState::new();
    let plane = state.data_plane.clone();
    plane
        .global_nodes
        .lock()
        .insert("tp".into(), transport_node("tp", "t1"));
    open_test_data(&state, "tp", &plane).await;
    assert!(state.transport.lock().await.is_open("tp"));

    // 模拟图重编译: tp 从全局节点表移除
    plane.global_nodes.lock().remove("tp");
    plane.reconcile().await;

    assert!(
        !state.transport.lock().await.is_open("tp"),
        "孤儿 Transport 连接应被关闭"
    );
    assert!(
        plane.raw_collectors.lock().is_empty(),
        "孤儿 Transport 的 raw 收集器应被移除"
    );
}

/// 节点仍被其他 tab 引用 (全局表中存在) → 连接与收集器保留
#[tokio::test]
async fn reconcile_keeps_node_referenced_by_other_tab() {
    let state = AppState::new();
    let plane = state.data_plane.clone();
    // t1 的图更新移除了 tp, 但 t2 仍引用同 id 节点 (合并视图按键存在)
    plane
        .global_nodes
        .lock()
        .insert("tp".into(), transport_node("tp", "t2"));
    open_test_data(&state, "tp", &plane).await;

    plane.reconcile().await;

    assert!(state.transport.lock().await.is_open("tp"));
    assert!(plane.raw_collectors.lock().contains_key("tp"));
}

/// Protocol 节点删除 → protocol_states / source_frames (sync) + buffers (reconcile) 清理
#[tokio::test]
async fn reconcile_cleans_protocol_buffers() {
    let state = AppState::new();
    let plane = state.data_plane.clone();
    plane
        .global_nodes
        .lock()
        .insert("pt".into(), protocol_node("pt", "t1"));
    plane.sync_protocol_states();
    plane.buffer_for("pt"); // 模拟产帧建的缓冲区
    assert!(plane.protocol_states.lock().contains_key("pt"));
    assert!(plane.buffers.lock().contains_key("pt"));

    // 模拟图重编译: pt 从全局节点表移除
    plane.global_nodes.lock().remove("pt");
    plane.sync_protocol_states();
    plane.reconcile().await;

    assert!(plane.protocol_states.lock().is_empty());
    assert!(plane.buffers.lock().is_empty());
}

/// Trigger 节点删除 → trigger_states 清理 (存活集来自各 tab 编译图的 Trigger 节点)
#[tokio::test]
async fn reconcile_cleans_trigger_states() {
    let state = AppState::new();
    let plane = state.data_plane.clone();
    // t1 图内含一个 Trigger 节点
    let g = CompiledGraph::compile(
        "t1".into(),
        vec![NodeDef {
            id: "tr1".into(),
            tab_id: "t1".into(),
            kind: NodeKind::Trigger {
                mode: "manual".into(),
                edge: "level".into(),
                default_miss: 0.0,
                default_miss_text: String::new(),
                command: String::new(),
                rules: vec![],
            },
        }],
        vec![],
    )
    .unwrap();
    plane.eval.graphs.lock().insert("t1".into(), g);
    {
        let mut ts = plane.eval.trigger_states.lock();
        ts.insert("tr1".into(), TriggerState::new(vec![], 0.0, String::new()));
        ts.insert(
            "tr_gone".into(),
            TriggerState::new(vec![], 0.0, String::new()),
        );
    }

    plane.reconcile().await;

    let ts = plane.eval.trigger_states.lock();
    assert!(ts.contains_key("tr1"), "存活 Trigger 节点状态应保留");
    assert!(!ts.contains_key("tr_gone"), "已删除 Trigger 节点状态应清理");
}
