//! HIR — 类型化图 (编译前端产物)
//!
//! 以 `petgraph::stable_graph::StableDiGraph` 为唯一图数据结构:
//! - 节点 id interning (`String` → `NodeIndex`), 后续平面/低阶全部以 `NodeIndex` 操作
//! - 双角色节点同槽共存: 同一 id 可能同时是全局 Protocol 定义 (字节平面, `byte_def`)
//!   与本 tab 的 ProtocolSource 引用 (数值平面, `value_def`)
//! - 端口域解析 + 边分类在构建时一次完成 ([`EdgeClass`]), 各平面据此
//!   投影出子图 (见 `plane` 模块)
//!
//! 容错语义: 边端点节点缺失时创建占位节点 (无双角色定义),
//! 端口域按 F32 处理。

use buffer_graph::Edge;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use rustc_hash::FxHashMap;

use node_kind::{port_domain, NodeDef, NodeKind, PortDomain, RAW_DATA_PORT_PREFIX};

use crate::errors::{port_domain_event, CompileError};

/// 节点图 HIR 类型
pub type Hir = StableDiGraph<HirNode, HirEdge>;

/// 边分类 — 构建时按两端端口域判定, 各平面据此投影
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeClass {
    /// 字节边 (两端端口域均为 Bytes) — 字节平面
    Byte,
    /// 数值边 (两端端口域均为 F32) — 值平面 (f32 槽位)
    F32,
    /// 字符串边 (两端端口域均为 String) — 值平面 (字符串槽位, 与 f32 共享拓扑序)
    Str,
    /// RawData 关联通道标记边 (Sink 的 `src:` 动态端口): 边只是用户意图标记,
    /// 字节/数值都不经 evaluate 流入 — 按源端域归类参与对应平面拓扑,
    /// 字节路由的默认分支忽略 (RawData 视图走订阅旁路);
    /// 字符串源不参与任何平面
    RawDataMarker(PortDomain),
}

impl EdgeClass {
    /// 是否参与字节平面拓扑
    pub const fn in_byte_plane(self) -> bool {
        matches!(self, Self::Byte | Self::RawDataMarker(PortDomain::Bytes))
    }

    /// 是否参与值平面拓扑 (f32 槽位 + 字符串槽位共享拓扑序)
    pub const fn in_value_plane(self) -> bool {
        matches!(
            self,
            Self::F32 | Self::Str | Self::RawDataMarker(PortDomain::F32)
        )
    }

    /// 是否为 f32 输入边 (参与 input_index 反查)
    pub const fn is_f32_input(self) -> bool {
        matches!(self, Self::F32 | Self::RawDataMarker(PortDomain::F32))
    }
}

/// HIR 节点权重 — 双角色定义同槽共存
#[derive(Debug, Default)]
pub struct HirNode {
    /// 节点 id (与 interning 表键一致, 供平面产物转回字符串)
    pub id: String,
    /// 数值平面定义 (ProtocolSource/Input/Math/Sink/...; None = 仅字节平面或占位节点)
    pub value_def: Option<NodeDef>,
    /// 字节平面定义 (Transport/Protocol)
    pub byte_def: Option<NodeDef>,
}

/// HIR 边权重 — 分类 + 端口句柄 (原始 source/target 由图端点持有)
#[derive(Debug)]
pub struct HirEdge {
    /// 边 id (诊断用)
    pub id: String,
    pub source_handle: String,
    pub target_handle: String,
    /// 构建期分类结果
    pub class: EdgeClass,
}

/// 类型化图 — 节点图编译的统一前端产物
pub struct TypedGraph {
    pub graph: Hir,
    /// id interning 表: 节点 id → NodeIndex
    index: FxHashMap<String, NodeIndex>,
}

impl TypedGraph {
    /// 构建 HIR: interning + 双角色定义归位 + 端口域解析 + 边分类
    ///
    /// 边两端端口域不一致时报 [`CompileError::DomainMismatch`]
    /// (RawData 关联通道边除外, 见 [`EdgeClass::RawDataMarker`])。
    pub fn build(
        nodes: impl IntoIterator<Item = NodeDef>,
        edges: impl IntoIterator<Item = Edge>,
    ) -> Result<Self, CompileError> {
        let mut graph = Hir::default();
        let mut index = FxHashMap::default();

        // 节点先行: 双角色定义归位 (同角色后到定义覆盖先到)
        for n in nodes {
            let ix = intern(&mut graph, &mut index, &n.id);
            let w = &mut graph[ix];
            if matches!(
                n.kind,
                NodeKind::Transport { .. } | NodeKind::Protocol { .. }
            ) {
                w.byte_def = Some(n);
            } else {
                w.value_def = Some(n);
            }
        }

        // 边: 端点 interning (缺失端点 → 占位节点, 域按 F32 容错) + 端口域分类
        for e in edges {
            let src = intern(&mut graph, &mut index, &e.source);
            let tgt = intern(&mut graph, &mut index, &e.target);
            let src_domain = domain_of(&graph, src, &e.source_handle, true);
            let tgt_domain = domain_of(&graph, tgt, &e.target_handle, false);
            let class = match (src_domain, tgt_domain) {
                (PortDomain::Bytes, PortDomain::Bytes) => EdgeClass::Byte,
                (PortDomain::F32, PortDomain::F32) => EdgeClass::F32,
                (PortDomain::String, PortDomain::String) => EdgeClass::Str,
                // RawData 关联通道边 (Sink 的 src:<source>:<handle> 动态端口):
                // 按源端域归类放行 (取代旧 LOOPBACK_IN_HANDLE 字符串特判)
                _ if is_raw_data_channel_target(&graph, tgt, &e.target_handle) => {
                    EdgeClass::RawDataMarker(src_domain)
                }
                // 其余组合 (String↔F32 / String↔Bytes 等跨域) 一律域不匹配
                _ => {
                    return Err(CompileError::DomainMismatch {
                        edge_id: e.id.clone(),
                        source_node: e.source.clone(),
                        source_port: e.source_handle.clone(),
                        src_domain: port_domain_event(src_domain),
                        target: e.target.clone(),
                        target_port: e.target_handle.clone(),
                        tgt_domain: port_domain_event(tgt_domain),
                    });
                }
            };
            graph.add_edge(
                src,
                tgt,
                HirEdge {
                    id: e.id,
                    source_handle: e.source_handle,
                    target_handle: e.target_handle,
                    class,
                },
            );
        }

        Ok(Self { graph, index })
    }

    /// 节点 id → NodeIndex
    pub fn node_index(&self, id: &str) -> Option<NodeIndex> {
        self.index.get(id).copied()
    }

    /// 节点 id → 数值平面定义
    pub fn value_def(&self, id: &str) -> Option<&NodeDef> {
        self.node_index(id)
            .and_then(|ix| self.graph[ix].value_def.as_ref())
    }

    /// 全部数值平面定义 (不含 Transport/Protocol 字节平面定义与占位节点)
    pub fn value_nodes(&self) -> impl Iterator<Item = &NodeDef> {
        self.graph
            .node_weights()
            .filter_map(|w| w.value_def.as_ref())
    }

    /// NodeIndex → 节点 id
    pub fn id_of(&self, ix: NodeIndex) -> &str {
        &self.graph[ix].id
    }
}

/// id interning: 不存在则建占位节点 (无双角色定义)
fn intern(graph: &mut Hir, index: &mut FxHashMap<String, NodeIndex>, id: &str) -> NodeIndex {
    if let Some(&ix) = index.get(id) {
        return ix;
    }
    let ix = graph.add_node(HirNode {
        id: id.to_string(),
        value_def: None,
        byte_def: None,
    });
    index.insert(id.to_string(), ix);
    ix
}

/// 端口域查询: 字节平面定义只在判定为 Bytes 时生效 (handle 命名两平面正交),
/// 否则回落数值平面定义; 占位节点 (端点缺失) 按 F32 处理
fn domain_of(graph: &Hir, ix: NodeIndex, handle: &str, is_output: bool) -> PortDomain {
    let w = &graph[ix];
    if let Some(bn) = &w.byte_def {
        let d = port_domain(&bn.kind, handle, is_output);
        if d == PortDomain::Bytes {
            return d;
        }
    }
    w.value_def
        .as_ref()
        .map_or(PortDomain::F32, |n| port_domain(&n.kind, handle, is_output))
}

/// 判定边目标是否为 RawData 控件的关联通道端口 (Sink + `src:` 动态端口 id)
/// RawData 是唯一使用 src: 端口约定的节点 (编译为 NodeKind::Sink);
/// 其他 Sink (Gauge/Command 等) 的端口不带此前缀, 跨域校验不受影响
fn is_raw_data_channel_target(graph: &Hir, target: NodeIndex, target_handle: &str) -> bool {
    target_handle.starts_with(RAW_DATA_PORT_PREFIX)
        && matches!(
            graph[target].value_def.as_ref().map(|n| &n.kind),
            Some(NodeKind::Sink)
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    #[test]
    fn test_intern_and_roles() {
        // 同 id 双角色: ProtocolSource (value) + Protocol (byte) 共存
        let g = TypedGraph::build(
            vec![
                make_protocol_source("pt", "t1", "pt", 2),
                make_protocol("pt"),
                make_math("m1", "t1", node_kind::MathOp::Add, 1),
            ],
            vec![],
        )
        .unwrap();
        let ix = g.node_index("pt").unwrap();
        assert!(g.graph[ix].value_def.is_some());
        assert!(g.graph[ix].byte_def.is_some());
        // 单角色
        let m = g.node_index("m1").unwrap();
        assert!(g.graph[m].value_def.is_some());
        assert!(g.graph[m].byte_def.is_none());
        // value_nodes 只含数值平面定义
        assert_eq!(g.value_nodes().count(), 2);
    }

    #[test]
    fn test_edge_classification() {
        let g = TypedGraph::build(
            vec![
                make_transport("tp"),
                make_protocol("pt"),
                make_decoder("dec", "t1"),
                make_math("m1", "t1", node_kind::MathOp::Add, 1),
                make_str("s1", "t1", node_kind::StrOp::Upper),
                make_str("s2", "t1", node_kind::StrOp::Len),
            ],
            vec![
                edge("e-byte", "tp", "rx", "pt", "in"),
                edge("e-byte2", "pt", "out", "dec", "in"),
                edge("e-f32", "dec", "value", "m1", "in0"),
                edge("e-str", "s1", "result", "s2", "str"),
            ],
        )
        .unwrap();
        let class_of = |id: &str| {
            g.graph
                .edge_weights()
                .find(|e| e.id == id)
                .map(|e| e.class)
                .unwrap()
        };
        assert_eq!(class_of("e-byte"), EdgeClass::Byte);
        assert_eq!(class_of("e-byte2"), EdgeClass::Byte);
        assert_eq!(class_of("e-f32"), EdgeClass::F32);
        assert_eq!(class_of("e-str"), EdgeClass::Str);
    }

    #[test]
    fn test_domain_mismatch() {
        // Protocol.out (Bytes) → Math.in0 (F32)
        let r = TypedGraph::build(
            vec![
                make_protocol("pt"),
                make_math("m1", "t1", node_kind::MathOp::Add, 1),
            ],
            vec![edge("e1", "pt", "out", "m1", "in0")],
        );
        assert!(matches!(r, Err(CompileError::DomainMismatch { .. })));
    }

    #[test]
    fn test_raw_data_marker_classified_by_source_domain() {
        let g = TypedGraph::build(
            vec![
                make_transport("tp"),
                make_protocol_source("pt", "t1", "pt", 1),
                make_protocol("pt"),
                make_sink("w-raw", "t1"),
            ],
            vec![
                edge("e-rx", "tp", "rx", "w-raw", "src:tp:rx"),
                edge("e-ch", "pt", "ch0", "w-raw", "src:pt:ch0"),
            ],
        )
        .unwrap();
        let class_of = |id: &str| {
            g.graph
                .edge_weights()
                .find(|e| e.id == id)
                .map(|e| e.class)
                .unwrap()
        };
        assert_eq!(
            class_of("e-rx"),
            EdgeClass::RawDataMarker(PortDomain::Bytes)
        );
        // 数值通道: (F32, F32) 同域, 先命中普通 F32 分类 (RawDataMarker 只承接跨域组合)
        assert_eq!(class_of("e-ch"), EdgeClass::F32);
        // 字节源标记边参与字节平面, 数值边参与值平面
        assert!(class_of("e-rx").in_byte_plane());
        assert!(!class_of("e-rx").in_value_plane());
        assert!(class_of("e-ch").in_value_plane());
        assert!(!class_of("e-ch").in_byte_plane());
    }

    #[test]
    fn test_missing_endpoint_tolerated() {
        // 边端点缺失 → 占位节点, 域按 F32, 编译不报错
        let g = TypedGraph::build(
            vec![make_math("m1", "t1", node_kind::MathOp::Add, 1)],
            vec![edge("e1", "ghost", "result", "m1", "in0")],
        )
        .unwrap();
        let ix = g.node_index("ghost").unwrap();
        assert!(g.graph[ix].value_def.is_none());
        assert!(g.graph[ix].byte_def.is_none());
    }

    #[test]
    fn test_raw_data_prefix_on_non_sink_rejected() {
        let r = TypedGraph::build(
            vec![
                make_protocol("pt"),
                make_math("m1", "t1", node_kind::MathOp::Add, 1),
            ],
            vec![edge("e1", "pt", "out", "m1", "src:pt:out")],
        );
        assert!(matches!(r, Err(CompileError::DomainMismatch { .. })));
    }
}
