//! 字节平面编译产物 (BytePlan)
//!
//! 字节平面与值平面 (f32/字符串槽位) 相互独立:
//! - 字节边 (两端端口域均为 Bytes, 或 RawData 字节标记边) 携带 `Vec<u8>`,
//!   事件驱动, 由调用方 (data_plane) 按 [`BytePlan::order`] 拓扑序逐节点 dispatch
//! - 跨平面不构成循环: 平面投影只看本平面边 (见 `plane` 模块), 结构性保证
//!
//! 字节平面节点 = Transport / Protocol 节点 ∪ 字节边端点
//! (FrameDecoder 字节入口 / widget loopbackOut 出口等)。

use std::collections::HashMap;

use petgraph::visit::{EdgeRef, IntoEdgeReferences};
use rustc_hash::FxHashSet;

use crate::plane::byte_plane_order;
use node_hir::CompileError;
use node_hir::TypedGraph;

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
    /// 平面成员集 (contains O(1) 查询)
    members: FxHashSet<String>,
}

impl BytePlan {
    /// 编译字节平面: 平面投影拓扑排序 (见 [`byte_plane_order`]) + 聚合 consumers
    ///
    /// 字节平面内循环 → [`CompileError::ByteCycle`] (完整环路径);
    /// 跨平面边不构成循环 (各平面只看本平面边)。
    ///
    /// consumers 覆盖全部字节平面边 (含占位端点; 路由查询时下游缺失由调用方容错)。
    pub fn build(g: &TypedGraph) -> Result<Self, CompileError> {
        // consumers: source → 下游路由
        let mut consumers: HashMap<String, Vec<ByteRoute>> = HashMap::new();
        for er in g.graph.edge_references() {
            if er.weight().class.in_byte_plane() {
                consumers
                    .entry(g.id_of(er.source()).to_string())
                    .or_default()
                    .push(ByteRoute {
                        target: g.id_of(er.target()).to_string(),
                        target_handle: er.weight().target_handle.clone(),
                    });
            }
        }

        let order: Vec<String> = byte_plane_order(g)?
            .iter()
            .map(|&ix| g.id_of(ix).to_string())
            .collect();
        let members: FxHashSet<String> = order.iter().cloned().collect();

        Ok(Self {
            order,
            consumers,
            members,
        })
    }

    /// 查询某字节源节点的下游路由 (无下游返回空切片)
    pub fn routes_for(&self, source: &str) -> &[ByteRoute] {
        self.consumers.get(source).map_or(&[], Vec::as_slice)
    }

    /// 节点是否属于字节平面 (O(1))
    pub fn contains(&self, node_id: &str) -> bool {
        self.members.contains(node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use node_testkit::*;

    #[test]
    fn test_byte_plan_topo_order() {
        // transport.rx → protocol.in, protocol.out → decoder.in
        let g = TypedGraph::build(
            vec![
                make_transport("tp"),
                make_protocol("pt"),
                make_decoder("dec", "t1"),
            ],
            vec![
                edge("tp-pt", "tp", "rx", "pt", "in"),
                edge("pt-dec", "pt", "out", "dec", "in"),
            ],
        )
        .unwrap();
        let plan = BytePlan::build(&g).expect("应编译成功");
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
        let g = TypedGraph::build(
            vec![make_protocol("pa"), make_protocol("pb")],
            vec![
                edge("pa-pb", "pa", "out", "pb", "in"),
                edge("pb-pa", "pb", "out", "pa", "in"),
            ],
        )
        .unwrap();
        let result = BytePlan::build(&g);
        match result {
            Err(CompileError::ByteCycle { cycle }) => {
                // 完整环路径: 首尾同节点, 两个协议节点都在环上
                assert_eq!(cycle.first(), cycle.last());
                assert!(cycle.contains(&"pa".to_string()));
                assert!(cycle.contains(&"pb".to_string()));
            }
            other => panic!("应报字节平面循环, 实际: {other:?}"),
        }
    }

    #[test]
    fn test_byte_plan_includes_isolated_transport() {
        // 无字节边的 Transport 节点也应在字节平面内 (事件驱动 dispatch 的起点)
        let g = TypedGraph::build(vec![make_transport("tp")], vec![]).unwrap();
        let plan = BytePlan::build(&g).expect("应编译成功");
        assert!(plan.contains("tp"));
        assert!(plan.routes_for("tp").is_empty());
    }
}
