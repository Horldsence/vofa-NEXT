//! 源图拓扑 op — `connect_edge` / `disconnect_edge` (连线拓扑的后端权威实现)
//!
//! 内置 AI (`ai`)、外部 MCP (`mcp_server`) 与前端提交共用同一条编译路径:
//! 克隆该 tab 源图 → 变更拓扑 → [`apply_tab_graph_parts`] 整体编译。
//! 编译失败 (环/端口域不匹配) 时源图不变、真实错误原样返回 ——
//! 错误边在结构上不可能存在;成功后 `graph:source` 事件把权威源图推回前端画布。
//!
//! 默认 handle 与 RawData `src:` 改写依据源图存储里的端口提示
//! ([`app_state::SourceNodeHint`] — widget 端口表形状由前端参数派生,
//! 后端经提示解析)。拓扑 op 不携带 widget 记录与位置 — 源图中现状保留。

use app_state::{
    Position, SourceGraphs, SourceNodeHint, TabSourceGraph, WidgetRecord, WorkspaceState,
};
use buffer_graph::Edge;
use error::ConfigError;
use kind::NodeKind;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use data_plane::DataPlaneState;
use tauri::AppHandle;
use vofa_core::{Error, Result};

use crate::apply_tab_graph_parts;

/// `graph:source` 事件名 — apply 成功后推送该 tab 的权威源图 (前端画布收敛依据)
pub const GRAPH_SOURCE_EVENT: &str = "graph:source";

/// [`GRAPH_SOURCE_EVENT`] 载荷 — 该 tab 最近一次成功编译的源图
/// (含 widget 配置记录与画布位置: 画布据此重建该 tab 完整视图)
#[derive(Debug, Clone, Serialize)]
pub struct GraphSourceEvent {
    pub tab_id: String,
    pub version: u64,
    pub nodes: Vec<kind::NodeDef>,
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub widgets: Vec<WidgetRecord>,
    #[serde(default)]
    pub positions: HashMap<String, Position>,
}

/// 连线 op 结果 — 新 (或已存在的等价) 边 id
#[derive(Debug, Clone, Serialize)]
pub struct ConnectedEdge {
    pub edge_id: String,
}

/// 删边 op 结果
#[derive(Debug, Clone, Serialize)]
pub struct DisconnectedEdge {
    pub edge_id: String,
    pub source: String,
    pub target: String,
}

/// 后端生成的边 id — `e-<时间hex>-<序号hex>` (与前端 nanoid 命名空间互不冲突)
fn next_edge_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!(
        "e-{:x}-{:x}",
        vofa_core::now_us(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// 定位连线所属 tab: 显式 tab_id 优先; 否则优先取同时持有 source 与 target 的
/// 字典序第一个 tab (全局节点存在于所有 tab, widget 节点只存在于归属 tab —
/// 两端都在才是正确归属); 退而求其次取持有 source 的第一个 tab。
/// 找不到 source 返回 `NodeNotFound`。
fn locate_tab(
    source_graphs: &SourceGraphs,
    tab_id: Option<&str>,
    source: &str,
    target: &str,
) -> Result<(String, TabSourceGraph)> {
    let store = source_graphs.lock();
    if let Some(tid) = tab_id {
        return Ok((tid.to_string(), store.get(tid).cloned().unwrap_or_default()));
    }
    let has = |g: &TabSourceGraph, id: &str| g.nodes.iter().any(|n| n.id == id);
    let mut both: Vec<&String> = store
        .iter()
        .filter(|(_, g)| has(g, source) && has(g, target))
        .map(|(tid, _)| tid)
        .collect();
    both.sort();
    let pick = both.into_iter().next().cloned().or_else(|| {
        let mut either: Vec<&String> = store
            .iter()
            .filter(|(_, g)| has(g, source))
            .map(|(tid, _)| tid)
            .collect();
        either.sort();
        either.first().map(|t| (*t).clone())
    });
    pick.map(|tid| {
        let g = store[&tid].clone();
        (tid, g)
    })
    .ok_or_else(|| {
        Error::Config(ConfigError::NodeNotFound {
            node_id: source.to_string(),
        })
    })
}

/// 解析源端口 handle: 显式指定 > 端口提示 > 按 NodeKind 兜底 (Transport rx / Protocol out)
fn resolve_source_handle(
    node: &kind::NodeDef,
    hint: Option<&SourceNodeHint>,
    given: Option<String>,
) -> Result<String> {
    if let Some(h) = given {
        return Ok(h);
    }
    if let Some(h) = hint.and_then(|h| h.default_output.clone()) {
        return Ok(h);
    }
    match &node.kind {
        NodeKind::Transport { .. } => Ok("rx".to_string()),
        NodeKind::Protocol { .. } => Ok("out".to_string()),
        _ => Err(Error::Config(ConfigError::GraphPortUnresolved {
            node_id: node.id.clone(),
            direction: "输出",
        })),
    }
}

/// 解析目标端口 handle: 显式指定 > 端口提示 > 按 NodeKind 兜底 (Protocol in)
fn resolve_target_handle(
    node: &kind::NodeDef,
    hint: Option<&SourceNodeHint>,
    given: Option<String>,
) -> Result<String> {
    if let Some(h) = given {
        return Ok(h);
    }
    if let Some(h) = hint.and_then(|h| h.default_input.clone()) {
        return Ok(h);
    }
    match &node.kind {
        NodeKind::Protocol { .. } => Ok("in".to_string()),
        _ => Err(Error::Config(ConfigError::GraphPortUnresolved {
            node_id: node.id.clone(),
            direction: "输入",
        })),
    }
}

/// 连线拓扑 op — 节点存在性校验 + 默认 handle + RawData 端口改写 + 等价边幂等
///
/// 编译失败源图不变;成功后源图与画布经 [`apply_tab_graph_parts`] 统一提交。
#[allow(clippy::too_many_arguments)]
#[allow(clippy::implicit_hasher)]
pub async fn apply_connect_edge(
    graphs: &Arc<parking_lot::Mutex<HashMap<String, engine::CompiledGraph>>>,
    graphs_version: &Arc<std::sync::atomic::AtomicU64>,
    data_plane: &DataPlaneState,
    source_graphs: &SourceGraphs,
    workspace: &WorkspaceState,
    app: Option<&AppHandle>,
    tab_id: Option<String>,
    source: String,
    target: String,
    source_handle: Option<String>,
    target_handle: Option<String>,
) -> Result<ConnectedEdge> {
    let (tab, graph) = locate_tab(source_graphs, tab_id.as_deref(), &source, &target)?;
    let src_def = graph.nodes.iter().find(|n| n.id == source).ok_or_else(|| {
        Error::Config(ConfigError::NodeNotFound {
            node_id: source.clone(),
        })
    })?;
    let tgt_def = graph.nodes.iter().find(|n| n.id == target).ok_or_else(|| {
        Error::Config(ConfigError::NodeNotFound {
            node_id: target.clone(),
        })
    })?;

    let sh = resolve_source_handle(src_def, graph.hints.get(&source), source_handle)?;
    // RawData 控件的输入端口动态派生 (`src:<source>:<handle>`) — 与前端 onConnect 同规则改写
    let th = if graph.hints.get(&target).is_some_and(|h| h.raw_data) {
        format!("src:{source}:{sh}")
    } else {
        resolve_target_handle(tgt_def, graph.hints.get(&target), target_handle)?
    };

    // 等价边幂等 (与前端 addEdge 去重语义一致)
    if let Some(existing) = graph.edges.iter().find(|e| {
        e.source == source && e.target == target && e.source_handle == sh && e.target_handle == th
    }) {
        return Ok(ConnectedEdge {
            edge_id: existing.id.clone(),
        });
    }

    let edge_id = next_edge_id();
    let mut edges = graph.edges.clone();
    edges.push(Edge {
        id: edge_id.clone(),
        source,
        source_handle: sh,
        target,
        target_handle: th,
    });
    // 拓扑 op 不携带 widget 记录与位置 — 源图中现状保留 (None 语义)
    apply_tab_graph_parts(
        graphs,
        graphs_version,
        data_plane,
        source_graphs,
        workspace,
        app,
        tab,
        graph.nodes,
        edges,
        graph.hints,
        None,
        None,
        None,
    )
    .await?;
    Ok(ConnectedEdge { edge_id })
}

/// 删边拓扑 op — 优先按 edge_id 精确查找, 否则按 source/target 组合 (可只给一端) 命中首条
#[allow(clippy::implicit_hasher)]
pub async fn apply_disconnect_edge(
    graphs: &Arc<parking_lot::Mutex<HashMap<String, engine::CompiledGraph>>>,
    graphs_version: &Arc<std::sync::atomic::AtomicU64>,
    data_plane: &DataPlaneState,
    source_graphs: &SourceGraphs,
    workspace: &WorkspaceState,
    app: Option<&AppHandle>,
    edge_id: Option<String>,
    source: Option<String>,
    target: Option<String>,
) -> Result<DisconnectedEdge> {
    let mut hit: Option<(String, Edge)> = None;
    {
        let store = source_graphs.lock();
        let mut tabs: Vec<(&String, &TabSourceGraph)> = store.iter().collect();
        tabs.sort_by(|a, b| a.0.cmp(b.0));
        'outer: for (tid, g) in tabs {
            for e in &g.edges {
                let id_match = edge_id.as_ref().map(|id| e.id == id.as_str());
                let src_ok = source.as_deref().is_none_or(|s| e.source == s);
                let tgt_ok = target.as_deref().is_none_or(|t| e.target == t);
                let matched = id_match.unwrap_or(src_ok && tgt_ok);
                if matched {
                    hit = Some((tid.clone(), e.clone()));
                    break 'outer;
                }
            }
        }
    }
    let (tab, edge) = hit.ok_or(Error::Config(ConfigError::GraphEdgeNotFound))?;
    let graph = source_graphs.lock().get(&tab).cloned().unwrap_or_default();
    let edges: Vec<Edge> = graph
        .edges
        .iter()
        .filter(|e| e.id != edge.id)
        .cloned()
        .collect();
    apply_tab_graph_parts(
        graphs,
        graphs_version,
        data_plane,
        source_graphs,
        workspace,
        app,
        tab,
        graph.nodes,
        edges,
        graph.hints,
        None,
        None,
        None,
    )
    .await?;
    Ok(DisconnectedEdge {
        edge_id: edge.id.clone(),
        source: edge.source.clone(),
        target: edge.target.clone(),
    })
}
