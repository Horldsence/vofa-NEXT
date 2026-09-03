//! 工作区存储 — widget 配置模型与 tab 元数据的后端权威
//!
//! 与 [`crate::SourceGraphs`](连线拓扑) 同级:源图存 NodeDef/Edge/端口提示,
//! 这里存 widget 配置记录 (kind + params 透传, schema 语义仍由前端类型定义)、
//! 画布位置与 tab 元数据。三者在 `apply_tab_graph_parts` 编译提交时原子更新,
//! 并整体落盘到 app config dir 的 `workspace.json` (防抖 + 退出 flush)。
//!
//! widget params 以 `serde_json::Value` 透传:后端是存储与分发权威
//! (持久化 / `graph:source` 分发 / 外部写入方提交),配置形状的校验发生在
//! 前端水合时 (未知 kind 剔除) — 避免在后端复刻 28 类控件的 schema。

use buffer_graph::Edge;
use kind::NodeDef;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 工作区文件名 (位于 app config dir)。
pub const WORKSPACE_FILE_NAME: &str = "workspace.json";

/// 画布坐标 — 与前端 React Flow `node.position` 同形。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

/// widget 配置记录 — 前端 `WidgetConfig` 的后端透传存储。
///
/// `id` 与节点 id 相同 (widget 即图节点);`params` 为该控件完整参数对象
/// (含 `id` 字段),后端不解释、原样回传。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetRecord {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub params: Value,
}

/// 控件 tab 元数据 — 名称与 widget 顺序 (配置本体在各 tab 源图的 `widgets`)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabMeta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub widgets: Vec<String>,
}

/// 数据面板 tab 元数据 — 透传存储 (面板类型校验在前端水合时做)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTabMeta {
    pub id: String,
    pub name: String,
    /// 面板类型 (与前端 `DataTabType` 同名透传)
    #[serde(rename = "type")]
    pub tab_type: String,
    #[serde(default = "default_true")]
    pub closable: bool,
    /// 派生面板的宿主 widget id (waveform/raw/... 跟随 widget;独立面板为 None)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget_id: Option<String>,
}

const fn default_true() -> bool {
    true
}

/// 工作区内存状态 (锁内)。
///
/// `restored` 标记启动时是否从磁盘恢复过工作区 — `workspace_get` 据此
/// 返回 None (全新安装, 前端走默认启动流程) 或完整快照。
#[derive(Default)]
pub struct WorkspaceInner {
    pub tabs: Vec<TabMeta>,
    pub data_tabs: Vec<DataTabMeta>,
    /// 全部节点的画布位置 (widget 节点按 tab 隔离, 全局节点跨 tab 共享 — 平铺按 id 索引)
    pub positions: HashMap<String, Position>,
    pub restored: bool,
    /// 自上次落盘后是否有变更 (防抖写入任务 / 退出 flush 的依据)
    pub dirty: bool,
}

/// 工作区存储句柄 — 全局单份 (与 [`crate::SourceGraphs`] 同生命周期)。
pub type WorkspaceState = Arc<parking_lot::Mutex<WorkspaceInner>>;

/// 单 tab 源图的落盘形态 (源图存储的序列化投影)。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TabGraphFile {
    pub nodes: Vec<NodeDef>,
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub hints: HashMap<String, crate::SourceNodeHint>,
    #[serde(default)]
    pub widgets: Vec<WidgetRecord>,
}

/// `workspace.json` 完整形态。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceFile {
    #[serde(default)]
    pub tabs: Vec<TabMeta>,
    #[serde(default)]
    pub data_tabs: Vec<DataTabMeta>,
    #[serde(default)]
    pub graphs: HashMap<String, TabGraphFile>,
    #[serde(default)]
    pub positions: HashMap<String, Position>,
}

/// 工作区文件完整路径。
pub fn workspace_path(dir: &Path) -> PathBuf {
    dir.join(WORKSPACE_FILE_NAME)
}

/// 读取工作区文件;文件不存在返回 None,损坏时告警并按不存在处理
/// (降级为全新启动, 下次成功提交会覆盖重写)。
pub fn load_workspace(dir: &Path) -> Option<WorkspaceFile> {
    let path = workspace_path(dir);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            log::warn!("workspace 读取失败 ({}): {e}", path.display());
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(file) => Some(file),
        Err(e) => {
            log::warn!("workspace 解析失败 ({}): {e}", path.display());
            None
        }
    }
}

/// 写入工作区文件 (全量覆盖)。
///
/// # Errors
/// 目录创建 / 序列化 / 写文件失败时返回 io 错误。
pub fn save_workspace(dir: &Path, file: &WorkspaceFile) -> std::io::Result<()> {
    let path = workspace_path(dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text =
        serde_json::to_string_pretty(file).map_err(|e| std::io::Error::other(e.to_string()))?;
    fs::write(&path, text)
}

/// 从内存状态收集落盘快照。
///
/// 两份状态分别在各自锁内克隆，绝不同时持锁。这样读快照无需依赖其他
/// 调用点的锁序，也不会与源图读取/提交路径形成 AB-BA 死锁。
pub fn collect_workspace_file(
    ws: &WorkspaceState,
    source_graphs: &crate::SourceGraphs,
) -> WorkspaceFile {
    let (tabs, data_tabs, positions) = {
        let ws = ws.lock();
        (ws.tabs.clone(), ws.data_tabs.clone(), ws.positions.clone())
    };
    let graphs = source_graphs
        .lock()
        .iter()
        .map(|(tab_id, g)| {
            (
                tab_id.clone(),
                TabGraphFile {
                    nodes: g.nodes.clone(),
                    edges: g.edges.clone(),
                    hints: g.hints.clone(),
                    widgets: g.widgets.clone(),
                },
            )
        })
        .collect();
    WorkspaceFile {
        tabs,
        data_tabs,
        graphs,
        positions,
    }
}

/// 清理已不存在节点的位置条目 (存活集合 = 全部 tab 源图节点),
/// 并在 `changed` 时标记落盘脏。
pub fn prune_positions(ws: &WorkspaceState, source_graphs: &crate::SourceGraphs) {
    let alive: std::collections::HashSet<String> = source_graphs
        .lock()
        .values()
        .flat_map(|g| g.nodes.iter().map(|n| n.id.clone()))
        .collect();
    let mut ws = ws.lock();
    let before = ws.positions.len();
    ws.positions.retain(|id, _| alive.contains(id));
    if ws.positions.len() != before {
        ws.dirty = true;
    }
}
