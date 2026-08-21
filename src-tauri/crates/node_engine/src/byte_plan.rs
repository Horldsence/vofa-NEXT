//! 字节平面编译产物 (BytePlan)
//!
//! 字节平面与数值平面 (f32 槽位) 相互独立:
//! - 字节边 (两端端口域均为 Bytes) 携带 `Vec<u8>`, 事件驱动, 由调用方
//!   (data_plane) 按 [`BytePlan::order`] 拓扑序逐节点 dispatch
//! - 跨平面不构成循环: f32 平面 DFS 只看 f32_edges, 本平面只看 byte_edges
//!
//! 字节平面节点 = Transport / Protocol 节点 ∪ 字节边端点
//! (FrameDecoder 字节入口 / widget loopbackOut 出口等)。

use std::collections::HashMap;

use buffer_graph::Edge;

use crate::CompileError;
use node_kind::{NodeDef, NodeKind};

/// 字节路由 — 一条字节边的下游端点
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteRoute {
    /// 下游节点 id
    pub target: String,
    /// 下游节点的字节输入 handle (如 "in" / "loopbackIn" / "tx")
    pub target_handle: String,
}

/// 字节平面处理计划
#[derive(Debug, Clone, Default)]
pub struct BytePlan {
    /// 字节节点处理顺序 (拓扑序, 上游在前)
    pub order: Vec<String>,
    /// 字节源节点 → 下游路由列表 (按 source 节点聚合)
    pub consumers: HashMap<String, Vec<ByteRoute>>,
}

impl BytePlan {
    /// 编译字节平面: 对字节边做三色 DFS 拓扑排序 + 聚合 consumers
    ///
    /// 字节平面内循环 → [`CompileError::ByteCycle`];
    /// 跨平面边不构成循环 (f32 平面只看 f32_edges)。
    pub fn build(
        nodes: &HashMap<String, NodeDef>,
        byte_edges: &[Edge],
    ) -> Result<Self, CompileError> {
        // consumers: source → 下游路由
        let mut consumers: HashMap<String, Vec<ByteRoute>> = HashMap::new();
        for e in byte_edges {
            consumers
                .entry(e.source.clone())
                .or_default()
                .push(ByteRoute {
                    target: e.target.clone(),
                    target_handle: e.target_handle.clone(),
                });
        }

        // 字节平面节点集: Transport/Protocol 节点 ∪ 字节边端点 (图中存在的)
        let mut byte_nodes: Vec<String> = nodes
            .values()
            .filter(|n| {
                matches!(
                    n.kind,
                    NodeKind::Transport { .. } | NodeKind::Protocol { .. }
                )
            })
            .map(|n| n.id.clone())
            .collect();
        for e in byte_edges {
            for id in [&e.source, &e.target] {
                if nodes.contains_key(id) && !byte_nodes.iter().any(|n| n == id) {
                    byte_nodes.push(id.clone());
                }
            }
        }
        // 排序保证确定性拓扑序 (HashMap 迭代序不稳定)
        byte_nodes.sort();

        // 三色 DFS (0=未访问, 1=访问中, 2=已完成), 后序即拓扑序
        let mut visited: HashMap<String, u8> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for id in &byte_nodes {
            dfs(id, byte_edges, nodes, &mut visited, &mut order)?;
        }

        Ok(Self { order, consumers })
    }

    /// 查询某字节源节点的下游路由 (无下游返回空切片)
    pub fn routes_for(&self, source: &str) -> &[ByteRoute] {
        self.consumers.get(source).map(Vec::as_slice).unwrap_or(&[])
    }

    /// 节点是否属于字节平面
    pub fn contains(&self, node_id: &str) -> bool {
        self.order.iter().any(|id| id == node_id)
    }
}

fn dfs(
    id: &str,
    byte_edges: &[Edge],
    nodes: &HashMap<String, NodeDef>,
    visited: &mut HashMap<String, u8>,
    order: &mut Vec<String>,
) -> Result<(), CompileError> {
    match visited.get(id) {
        Some(&1) => return Err(CompileError::ByteCycle),
        Some(&2) => return Ok(()),
        _ => {}
    }
    visited.insert(id.to_string(), 1);
    for e in byte_edges {
        if e.target == id && nodes.contains_key(&e.source) {
            dfs(&e.source, byte_edges, nodes, visited, order)?;
        }
    }
    visited.insert(id.to_string(), 2);
    order.push(id.to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema_types::ProtocolConfig;
use vofa_core::config::TransportConfig;

    fn transport(id: &str) -> NodeDef {
        NodeDef {
            id: id.into(),
            tab_id: "t1".into(),
            kind: NodeKind::Transport {
                config: TransportConfig::TestData(Default::default()),
            },
        }
    }

    fn protocol(id: &str) -> NodeDef {
        NodeDef {
            id: id.into(),
            tab_id: "t1".into(),
            kind: NodeKind::Protocol {
                config: ProtocolConfig::default(),
                convert_to: None,
                schema: None,
            },
        }
    }

    fn decoder(id: &str) -> NodeDef {
        NodeDef {
            id: id.into(),
            tab_id: "t1".into(),
            kind: NodeKind::FrameDecoder {
                blocks: vec![],
                enable_valid: false,
                enable_frame_count: false,
                enable_last_timestamp: false,
                enable_fps: false,
                loopback: false,
            },
        }
    }

    fn edge(src: &str, src_h: &str, tgt: &str, tgt_h: &str) -> Edge {
        Edge {
            id: format!("{}-{}", src, tgt),
            source: src.into(),
            source_handle: src_h.into(),
            target: tgt.into(),
            target_handle: tgt_h.into(),
        }
    }

    #[test]
    fn test_byte_plan_topo_order() {
        // transport.rx → protocol.in, protocol.out → decoder.in
        let nodes: HashMap<String, NodeDef> = [transport("tp"), protocol("pt"), decoder("dec")]
            .into_iter()
            .map(|n| (n.id.clone(), n))
            .collect();
        let edges = vec![edge("tp", "rx", "pt", "in"), edge("pt", "out", "dec", "in")];
        let plan = BytePlan::build(&nodes, &edges).expect("应编译成功");
        let pos = |id: &str| plan.order.iter().position(|n| n == id).unwrap();
        assert!(pos("tp") < pos("pt"), "transport 应先于 protocol");
        assert!(pos("pt") < pos("dec"), "protocol 应先于 decoder");

        // consumers 聚合
        let routes = plan.routes_for("tp");
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].target, "pt");
        assert_eq!(routes[0].target_handle, "in");
        assert_eq!(plan.routes_for("pt").len(), 1);
        assert!(plan.routes_for("dec").is_empty());
    }

    #[test]
    fn test_byte_plan_cycle_detected() {
        // protocol_a.out → protocol_b.in, protocol_b.out → protocol_a.in
        let nodes: HashMap<String, NodeDef> = [protocol("pa"), protocol("pb")]
            .into_iter()
            .map(|n| (n.id.clone(), n))
            .collect();
        let edges = vec![edge("pa", "out", "pb", "in"), edge("pb", "out", "pa", "in")];
        let result = BytePlan::build(&nodes, &edges);
        assert!(matches!(result, Err(CompileError::ByteCycle)));
    }

    #[test]
    fn test_byte_plan_includes_isolated_transport() {
        // 无字节边的 Transport 节点也应在字节平面内 (事件驱动 dispatch 的起点)
        let nodes: HashMap<String, NodeDef> = [transport("tp")]
            .into_iter()
            .map(|n| (n.id.clone(), n))
            .collect();
        let plan = BytePlan::build(&nodes, &[]).expect("应编译成功");
        assert!(plan.contains("tp"));
        assert!(plan.routes_for("tp").is_empty());
    }
}
