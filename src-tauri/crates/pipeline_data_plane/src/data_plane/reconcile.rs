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

use node_kind::NodeKind;

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
