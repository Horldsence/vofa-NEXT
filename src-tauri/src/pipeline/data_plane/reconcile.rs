//! 孤儿资源清理 — 图重编译 (update_tab_graph / remove_tab_graph) 提交后调用
//!
//! 全局节点表是所有 tab 的合并视图: 节点仍被任一 tab 引用时保留,
//! 只有所有 tab 都不再引用时 (键从全局表消失) 才清理运行时资源:
//! - Transport 节点: registry 仍 is_open 但全局表已不存在 → detach 读任务 + close 连接
//! - Protocol 节点: protocol_states / source_frames 由 `sync_protocol_states` 清理,
//!   本模块补清 buffers (key = Protocol 节点 id)
//! - raw_collectors (key = Transport 节点 id): 无对应 Transport 节点的键移除
//! - 悬空 ProtocolSource (引用的 node_id 不是全局表中的 Protocol 节点):
//!   不编译失败, 仅 log::warn (按 graphs_version 去重, 每个图版本最多警告一次)

use std::collections::HashSet;
use std::sync::atomic::Ordering;

use vofa_next_nodes::NodeKind;

use super::DataPlaneState;

/// 全局节点表快照: Transport 节点 id 集 / Protocol 节点 id 集 /
/// 悬空 ProtocolSource 列表 (ProtocolSource 节点 id, 引用的 node_id)
struct NodeSets {
    transports: HashSet<String>,
    protocols: HashSet<String>,
    dangling_sources: Vec<(String, String)>,
}

/// 提取 reconcile 所需的键集 (锁内一次性快照):
/// Transport/Protocol 集合来自全局节点表; ProtocolSource 是 tab 数值平面的
/// 帧源引用 (不进全局表), 悬空检测扫描各 tab 编译图的节点表。
fn snapshot_node_sets(plane: &DataPlaneState) -> NodeSets {
    let nodes = plane.global_nodes.lock();
    let mut sets = NodeSets {
        transports: HashSet::new(),
        protocols: HashSet::new(),
        dangling_sources: Vec::new(),
    };
    for n in nodes.values() {
        match &n.kind {
            NodeKind::Transport { .. } => {
                sets.transports.insert(n.id.clone());
            }
            NodeKind::Protocol { .. } => {
                sets.protocols.insert(n.id.clone());
            }
            _ => {}
        }
    }
    // 悬空 ProtocolSource: 引用的 node_id 不是全局表中的 Protocol 节点
    for g in plane.eval.graphs.lock().values() {
        for n in g.nodes().values() {
            if let NodeKind::ProtocolSource { node_id, .. } = &n.kind {
                let target_is_protocol = matches!(
                    nodes.get(node_id).map(|t| &t.kind),
                    Some(NodeKind::Protocol { .. })
                );
                if !target_is_protocol {
                    sets.dangling_sources.push((n.id.clone(), node_id.clone()));
                }
            }
        }
    }
    sets
}

impl DataPlaneState {
    /// 图重编译后的孤儿资源清理 (幂等, 每次 update/remove_tab_graph 提交后调用)
    ///
    /// 调用前应先执行 `sync_protocol_states` (protocol_states / source_frames 清理)。
    pub async fn reconcile(&self) {
        let sets = snapshot_node_sets(self);

        // 悬空 ProtocolSource 警告 (不编译失败; 按 graphs_version 去重避免刷屏)
        if !sets.dangling_sources.is_empty() {
            let version = self.eval.graphs_version.load(Ordering::Relaxed);
            if self.reconcile_warn_version.swap(version, Ordering::Relaxed) != version {
                for (src, target) in &sets.dangling_sources {
                    log::warn!(
                        "ProtocolSource 节点 {} 引用的 Protocol 节点 {} 不存在 (已悬空, 输出保持最新缓存值)",
                        src,
                        target
                    );
                }
            }
        }

        // 孤儿 Transport: registry 仍 open 但全局表已无该 Transport 节点
        // (所有 tab 都不再引用) → detach 读任务 + close 连接
        let orphans: Vec<String> = {
            let manager = self.transport.lock().await;
            manager
                .list_open()
                .into_iter()
                .filter(|id| !sets.transports.contains(id))
                .collect()
        };
        if !orphans.is_empty() {
            let mut manager = self.transport.lock().await;
            for id in orphans {
                self.detach(&id);
                manager.close(&id);
                log::info!("图重编译: 清理孤儿传输连接 {}", id);
            }
        }

        // buffers: 仅保留仍存在的 Protocol 节点键
        self.buffers
            .lock()
            .retain(|id, _| sets.protocols.contains(id));
        // raw_collectors: 仅保留仍存在的 Transport 节点键
        self.raw_collectors
            .lock()
            .retain(|id, _| sets.transports.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use vofa_next_core::{ProtocolConfig, TransportConfig};
    use vofa_next_nodes::NodeDef;

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

    async fn open_test_data(state: &AppState, id: &str) {
        state
            .transport
            .lock()
            .await
            .open(
                id,
                TransportConfig::TestData(Default::default()),
                vofa_next_core::TestDataLink::new(ProtocolConfig::RawData),
            )
            .await
            .unwrap();
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
        open_test_data(&state, "tp").await;
        plane.raw_collector_for("tp");
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
        open_test_data(&state, "tp").await;
        plane.raw_collector_for("tp");

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

    /// 悬空 ProtocolSource 检测: 引用的 node_id 不是 Protocol 节点
    /// (ProtocolSource 是 tab 数值平面节点, 扫描各 tab 编译图)
    #[test]
    fn dangling_protocol_source_detected() {
        let state = AppState::new();
        let plane = state.data_plane.clone();
        plane
            .global_nodes
            .lock()
            .insert("pt".into(), protocol_node("pt", "t1"));

        let psrc = |id: &str, target: &str| NodeDef {
            id: id.into(),
            tab_id: "t1".into(),
            kind: NodeKind::ProtocolSource {
                node_id: target.into(),
                channels: 2,
                port_names: None,
            },
        };
        let graph = vofa_next_nodes::CompiledGraph::compile(
            "t1".into(),
            vec![psrc("src_ok", "pt"), psrc("src_bad", "ghost")],
            vec![],
        )
        .unwrap();
        plane.eval.graphs.lock().insert("t1".into(), graph);

        let sets = snapshot_node_sets(&plane);
        assert_eq!(sets.dangling_sources.len(), 1);
        assert_eq!(
            sets.dangling_sources[0],
            ("src_bad".to_string(), "ghost".to_string())
        );
    }
}
