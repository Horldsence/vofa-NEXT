//! 工作区命令 — 水合快照读取 / tab 元数据提交 / 启动恢复
//!
//! widget 配置模型与持久化的后端入口:
//! - `workspace_get`: 前端启动时拉取权威快照 (tabs + data_tabs + 各 tab 源图 +
//!   widget 记录 + 画布位置);未恢复过持久化工作区时返回 None (前端走默认启动)
//! - `workspace_set_tabs`: 控件 tab 与数据面板 tab 元数据提交 (增删改名后整表覆盖)
//! - [`restore_workspace`]: 启动时从 `workspace.json` 恢复存储并逐 tab 重编译,
//!   让图在后端立即可求值 (前端随后水合, 不再回推整图)

use app_state::{load_workspace, DataTabMeta, Position, TabGraphFile, TabMeta, WidgetRecord};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::Ordering;
use tauri::State;

use app_state::AppState;
use buffer_graph::Edge;
use node_kind::NodeDef;

use crate::apply_tab_graph_parts;

/// `workspace_get` 响应 — 前端水合快照 (图形与 `graph:source` 事件同构,
/// 另带 tab 元数据与全局版本号)
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceSnapshot {
    pub version: u64,
    pub tabs: Vec<TabMeta>,
    pub data_tabs: Vec<DataTabMeta>,
    pub graphs: Vec<TabGraphSnapshot>,
    pub positions: HashMap<String, Position>,
}

/// 单 tab 快照 — 权威源图 + widget 配置记录
#[derive(Debug, Clone, Serialize)]
pub struct TabGraphSnapshot {
    pub tab_id: String,
    pub nodes: Vec<NodeDef>,
    pub edges: Vec<Edge>,
    pub widgets: Vec<WidgetRecord>,
}

/// 读取工作区水合快照;启动时未恢复过持久化工作区返回 None。
#[tauri::command]
pub fn workspace_get(state: State<'_, AppState>) -> Option<WorkspaceSnapshot> {
    let ws = state.workspace.lock();
    if !ws.restored {
        return None;
    }
    let (tabs, data_tabs, positions) =
        (ws.tabs.clone(), ws.data_tabs.clone(), ws.positions.clone());
    drop(ws);
    let graphs = {
        let store = state.source_graphs.lock();
        // tab 元数据顺序优先, 孤儿源图 (tab 已删但未清理) 按字典序补后
        let mut ordered: Vec<TabGraphSnapshot> = Vec::new();
        for tab in &tabs {
            if let Some(g) = store.get(&tab.id) {
                ordered.push(TabGraphSnapshot {
                    tab_id: tab.id.clone(),
                    nodes: g.nodes.clone(),
                    edges: g.edges.clone(),
                    widgets: g.widgets.clone(),
                });
            }
        }
        let mut rest: Vec<&String> = store.keys().collect();
        rest.sort();
        for tab_id in rest {
            if tabs.iter().any(|t| &t.id == tab_id) {
                continue;
            }
            let g = &store[tab_id];
            ordered.push(TabGraphSnapshot {
                tab_id: tab_id.clone(),
                nodes: g.nodes.clone(),
                edges: g.edges.clone(),
                widgets: g.widgets.clone(),
            });
        }
        ordered
    };
    let version = state.graphs_version.load(Ordering::Relaxed);
    Some(WorkspaceSnapshot {
        version,
        tabs,
        data_tabs,
        graphs,
        positions,
    })
}

/// 提交 tab 元数据 (控件 tab + 数据面板 tab) — 增删/改名/重排后整表覆盖。
#[tauri::command]
pub fn workspace_set_tabs(
    state: State<'_, AppState>,
    tabs: Vec<TabMeta>,
    data_tabs: Vec<DataTabMeta>,
) {
    let mut ws = state.workspace.lock();
    ws.tabs = tabs;
    ws.data_tabs = data_tabs;
    ws.dirty = true;
}

/// 启动恢复 — 从 app config dir 的 `workspace.json` 载入工作区并逐 tab 重编译。
///
/// 返回是否成功恢复 (文件不存在或损坏返回 false, 前端据此走默认启动流程)。
/// 重编译复用 [`apply_tab_graph_parts`] 同一提交入口 (不发事件 — 前端尚未就绪,
/// 水合通过 `workspace_get` 拉取),恢复产生的版本号递增即前端的水合基线。
pub async fn restore_workspace(state: &AppState, dir: &Path) -> bool {
    let Some(file) = load_workspace(dir) else {
        return false;
    };
    let app_state::WorkspaceFile {
        tabs,
        data_tabs,
        graphs,
        positions,
    } = file;
    {
        let mut ws = state.workspace.lock();
        ws.tabs = tabs;
        ws.data_tabs = data_tabs;
        ws.positions = positions;
        ws.restored = true;
    }
    for (tab_id, g) in graphs {
        let TabGraphFile {
            nodes,
            edges,
            hints,
            widgets,
        } = g;
        if let Err(e) = apply_tab_graph_parts(
            &state.graphs,
            &state.graphs_version,
            &state.data_plane,
            &state.source_graphs,
            &state.workspace,
            None,
            tab_id,
            nodes,
            edges,
            hints,
            Some(widgets),
            None,
            None,
        )
        .await
        {
            // 单 tab 恢复失败 (如图结构已损坏) 不阻塞整体启动 — 该 tab 按空处理
            log::warn!("工作区 tab 图恢复失败: {e}");
        }
    }
    // 启动恢复本身不触发重写 (dirty 由 apply 置位, 此处清零)
    state.workspace.lock().dirty = false;
    true
}
