//! HIR 查询 — 将编译期 TypedGraph 投影为前端可消费的序列化视图
//!
//! 提供 [`get_graph_hir`] Tauri 命令, 供 `compile-results` Tab 按需拉取后端
//! 编译产物 (边分类 / 端口域 / 节点双角色). 数据源是 `state.graphs` 里缓存的
//! `CompiledGraph.hir` (`TypedGraph`).
//!
//! 与现有的 [`crate::derived`] 模块的关系:
//! - `derived` 计算"节点输出端口表" (每节点对外端口 + 通道数), 走 `graph:derived` 事件流
//! - `hir_query` 暴露"全图 HIR 视图" (节点 + 边 + 端口域 + 边分类), 走 `get_graph_hir` 命令拉取
//! 两者各司其职, 无重复字段, 也不互相替代.

use node_engine::{CompiledGraph, EdgeClass, HirNode, TypedGraph};
use node_kind::{port_domain, NodeDef, PortDomain};
use petgraph::visit::{EdgeRef, IntoEdgeReferences};
use serde::Serialize;
use tauri::State;

use app_state::AppState;
use vofa_core::Result;

/// 端口域 wire 形态 — 与 `derived::DerivedPortDomain` 一致 (`PascalCase` 序列化)
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum PortDomainView {
    F32,
    Bytes,
    String,
}

impl From<PortDomain> for PortDomainView {
    fn from(d: PortDomain) -> Self {
        match d {
            PortDomain::F32 => Self::F32,
            PortDomain::Bytes => Self::Bytes,
            PortDomain::String => Self::String,
        }
    }
}

/// 边分类 wire 形态 — mirror `node_engine::hir::EdgeClass`
/// (`tag = "kind"` + `snake_case` 变体名, 与前端 `kind` 字段对齐)
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HirEdgeClassView {
    Byte,
    F32,
    Str,
    RawDataMarker { source_domain: PortDomainView },
}

impl From<EdgeClass> for HirEdgeClassView {
    fn from(c: EdgeClass) -> Self {
        match c {
            EdgeClass::Byte => Self::Byte,
            EdgeClass::F32 => Self::F32,
            EdgeClass::Str => Self::Str,
            EdgeClass::RawDataMarker(d) => Self::RawDataMarker {
                source_domain: d.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HirEdgeView {
    pub edge_id: String,
    pub source_node: String,
    pub source_handle: String,
    pub source_domain: PortDomainView,
    pub target_node: String,
    pub target_handle: String,
    pub target_domain: PortDomainView,
    pub class: HirEdgeClassView,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HirNodeView {
    pub node_id: String,
    pub has_value_def: bool,
    pub has_byte_def: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GraphHir {
    pub tab_id: String,
    pub nodes: Vec<HirNodeView>,
    pub edges: Vec<HirEdgeView>,
}

/// 从 `CompiledGraph` 提取 HIR 视图; 编译未完成 (无 CompiledGraph) 返回空表
///
/// 锁持续时间: 仅 `state.graphs` HashMap lookup 期间持锁; 迭代 `typed.graph`
/// 在锁外执行 (lock guard 在 `extract_hir` 入口已 drop).
pub fn extract_hir(tab_id: &str, graph: Option<&CompiledGraph>) -> GraphHir {
    let Some(g) = graph else {
        return GraphHir {
            tab_id: tab_id.to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
        };
    };
    let typed: &TypedGraph = g.hir();
    let nodes: Vec<HirNodeView> = typed
        .graph
        .node_indices()
        .map(|ix| {
            let w = &typed.graph[ix];
            HirNodeView {
                node_id: w.id.clone(),
                has_value_def: w.value_def.is_some(),
                has_byte_def: w.byte_def.is_some(),
            }
        })
        .collect();
    let edges: Vec<HirEdgeView> = typed
        .graph
        .edge_references()
        .map(|er| {
            let src_ix = er.source();
            let tgt_ix = er.target();
            let weight = er.weight();
            let src_node_id = typed.id_of(src_ix).to_string();
            let tgt_node_id = typed.id_of(tgt_ix).to_string();
            let src_node_w = &typed.graph[src_ix];
            let tgt_node_w = &typed.graph[tgt_ix];
            let src_domain = port_domain_of_node(src_node_w, &weight.source_handle, true);
            let tgt_domain = port_domain_of_node(tgt_node_w, &weight.target_handle, false);
            HirEdgeView {
                edge_id: weight.id.clone(),
                source_node: src_node_id,
                source_handle: weight.source_handle.clone(),
                source_domain: src_domain.into(),
                target_node: tgt_node_id,
                target_handle: weight.target_handle.clone(),
                target_domain: tgt_domain.into(),
                class: weight.class.into(),
            }
        })
        .collect();
    GraphHir {
        tab_id: tab_id.to_string(),
        nodes,
        edges,
    }
}

/// 端口域解析 (mirror `node_engine::hir::domain_of`):
/// 优先查 byte_def (handle 命名两平面正交, 字节判定优先); 否则回落 value_def;
/// 占位节点 (无双双角色定义) 按 F32 处理.
///
/// 与 `node_engine::hir::domain_of` 行为一致; 该函数暂未 pub,
/// `hir_query` 在 cmd_graph crate 内独立维护一份以避免反向修改 `node_engine`
/// 公共 API.
fn port_domain_of_node(node: &HirNode, handle: &str, is_output: bool) -> PortDomain {
    if let Some(bn) = &node.byte_def {
        let d = port_domain(&bn.kind, handle, is_output);
        if d == PortDomain::Bytes {
            return d;
        }
    }
    node.value_def
        .as_ref()
        .map_or(PortDomain::F32, |n: &NodeDef| {
            port_domain(&n.kind, handle, is_output)
        })
}

/// Tauri 命令: 按 tab 拉取 HIR 视图 (供 `compile-results` Tab 渲染)
///
/// 注: `state.graphs` 仅在 `apply_tab_graph` 编译成功时插入 — 编译未完成
/// 或编译失败时返回空表 (前端会保留上次成功的 HIR 显示并展示占位文案).
#[tauri::command]
pub async fn get_graph_hir(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<GraphHir> {
    let graphs = state.graphs.lock();
    let view = extract_hir(&tab_id, graphs.get(&tab_id));
    drop(graphs);
    Ok(view)
}

// ============ 测试 ============

#[cfg(test)]
mod tests {
    use super::*;
    use node_kind::{NodeDef, NodeKind};

    fn math_def(id: &str, tab_id: &str) -> NodeDef {
        NodeDef {
            id: id.into(),
            tab_id: tab_id.into(),
            kind: NodeKind::Math {
                op: node_kind::MathOp::Add,
                input_count: 1,
            },
        }
    }

    fn transport_def(id: &str) -> NodeDef {
        NodeDef {
            id: id.into(),
            tab_id: "main".into(),
            kind: NodeKind::Transport {
                config: vofa_core::config::TransportConfig::TestData(Default::default()),
            },
        }
    }

    fn protocol_def(id: &str) -> NodeDef {
        NodeDef {
            id: id.into(),
            tab_id: "main".into(),
            kind: NodeKind::Protocol {
                config: schema_types::ProtocolConfig::JustFloat { channels: None },
                convert_to: None,
                schema: None,
            },
        }
    }

    fn input_def(id: &str, tab_id: &str) -> NodeDef {
        NodeDef {
            id: id.into(),
            tab_id: tab_id.into(),
            kind: NodeKind::Input,
        }
    }

    /// 编译成功 → HIR 含全部节点 + 边
    #[test]
    fn extract_hir_returns_classified_edges() {
        let nodes = vec![input_def("in1", "t1"), math_def("m1", "t1")];
        let edges = vec![buffer_graph::Edge {
            id: "e1".into(),
            source: "in1".into(),
            source_handle: "value".into(),
            target: "m1".into(),
            target_handle: "in0".into(),
        }];
        let compiled = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        let view = extract_hir("t1", Some(&compiled));
        assert_eq!(view.tab_id, "t1");
        assert_eq!(view.nodes.len(), 2);
        assert_eq!(view.edges.len(), 1, "编译时含 1 条边, HIR 视图应保留");
        assert_eq!(view.edges[0].edge_id, "e1");
        assert_eq!(view.edges[0].class, HirEdgeClassView::F32);
    }

    /// 编译未完成 (无 CompiledGraph) → 返回空表
    #[test]
    fn extract_hir_returns_empty_when_no_graph() {
        let view = extract_hir("t1", None);
        assert_eq!(view.tab_id, "t1");
        assert!(view.nodes.is_empty());
        assert!(view.edges.is_empty());
    }

    /// 字节边分类: Transport.out → Protocol.in (两端 Bytes 域)
    #[test]
    fn extract_hir_byte_edge_class() {
        let nodes = vec![transport_def("tp"), protocol_def("pt")];
        let edges = vec![buffer_graph::Edge {
            id: "e-byte".into(),
            source: "tp".into(),
            source_handle: "rx".into(),
            target: "pt".into(),
            target_handle: "in".into(),
        }];
        let compiled = CompiledGraph::compile("main".into(), nodes, edges).unwrap();
        let view = extract_hir("main", Some(&compiled));
        let edge = view.edges.iter().find(|e| e.edge_id == "e-byte").unwrap();
        assert_eq!(edge.class, HirEdgeClassView::Byte);
        assert_eq!(edge.source_domain, PortDomainView::Bytes);
        assert_eq!(edge.target_domain, PortDomainView::Bytes);
    }

    /// 双角色节点: ProtocolSource (value) + Protocol (byte) 同 id 共存
    #[test]
    fn extract_hir_dual_role_node() {
        let nodes = vec![
            NodeDef {
                id: "pt".into(),
                tab_id: "t1".into(),
                kind: node_kind::NodeKind::ProtocolSource {
                    node_id: "pt".into(),
                    channels: 1,
                    port_names: None,
                },
            },
            protocol_def("pt"),
        ];
        let compiled = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
        let view = extract_hir("t1", Some(&compiled));
        let pt_node = view.nodes.iter().find(|n| n.node_id == "pt").unwrap();
        assert!(pt_node.has_value_def, "ProtocolSource 是数值平面定义");
        assert!(pt_node.has_byte_def, "Protocol 是字节平面定义");
    }
}