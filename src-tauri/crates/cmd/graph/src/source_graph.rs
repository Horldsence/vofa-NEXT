//! 源图拓扑 op Tauri 命令 — 薄适配层
//!
//! 拓扑核心 (默认 handle 解析 / RawData 改写 / 幂等去重) 在 [`graph_ops`],
//! 前端、内置 AI 与外部 MCP 共用同一条编译路径。

use app_state::{AppState, Position};
use graph_ops::{
    apply_connect_edge, apply_disconnect_edge, ConnectedEdge, DisconnectedEdge, GraphSourceEvent,
};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};
use vofa_core::Result;

/// 连线 — 连线拓扑的后端权威入口 (内置 AI / 外部 MCP 共用)。
///
/// handle 省略时按端口提示或节点类型补默认;RawData 控件目标自动改写
/// `src:<source>:<handle>`。编译失败 (环/端口域不匹配) 返回真实原因, 源图不变。
#[tauri::command]
pub async fn connect_edge(
    state: State<'_, AppState>,
    app: AppHandle,
    source: String,
    target: String,
    tab_id: Option<String>,
    source_handle: Option<String>,
    target_handle: Option<String>,
) -> Result<ConnectedEdge> {
    apply_connect_edge(
        &state.graphs,
        &state.graphs_version,
        &state.data_plane,
        &state.source_graphs,
        &state.workspace,
        Some(&app),
        tab_id,
        source,
        target,
        source_handle,
        target_handle,
    )
    .await
}

/// 读取指定 tab 的权威源图 (版本冲突后前端拉取合并重试; tab 无源图时返回 null)
#[tauri::command]
pub fn get_source_graph(state: State<'_, AppState>, tab_id: String) -> Option<GraphSourceEvent> {
    // 两份快照分段获取，禁止嵌套 workspace/source_graphs 锁。
    let positions_all = state.workspace.lock().positions.clone();
    let g = state.source_graphs.lock().get(&tab_id)?.clone();
    let positions: HashMap<String, Position> = positions_all
        .into_iter()
        .filter(|(id, _)| g.nodes.iter().any(|n| n.id == id.as_str()))
        .collect();
    Some(GraphSourceEvent {
        tab_id,
        version: state.graphs_version.load(Ordering::Relaxed),
        nodes: g.nodes,
        edges: g.edges,
        widgets: g.widgets,
        positions,
    })
}

/// 删线 — 按 edge_id 或 source/target (可只给一端) 查找删除。
#[tauri::command]
pub async fn disconnect_edge(
    state: State<'_, AppState>,
    app: AppHandle,
    edge_id: Option<String>,
    source: Option<String>,
    target: Option<String>,
) -> Result<DisconnectedEdge> {
    apply_disconnect_edge(
        &state.graphs,
        &state.graphs_version,
        &state.data_plane,
        &state.source_graphs,
        &state.workspace,
        Some(&app),
        edge_id,
        source,
        target,
    )
    .await
}
