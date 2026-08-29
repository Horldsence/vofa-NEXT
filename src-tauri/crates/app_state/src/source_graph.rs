//! 源图存储 — 连线拓扑的后端权威 (仅存类型, op 逻辑在 `cmd_graph::source_graph`)
//!
//! 前端整图提交 (`update_tab_graph`) 与后端拓扑 op (`connect_edge` / `disconnect_edge`)
//! 共同维护:apply 编译成功后写入, 编译失败不落盘 (旧图保留)。
//! widget 参数与画布位置仍归前端所有 —— 端口提示 [`SourceNodeHint`] 随 sync 附带,
//! 供后端 op 解析默认 handle 与 RawData `src:` 端口改写 (后端无法枚举 Sink 端口)。

use buffer_graph::Edge;
use node_kind::NodeDef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// 节点端口提示 — 前端 sync 时按节点 id 附带
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceNodeHint {
    /// 默认输入端口 (连线 target 省略 target_handle 时使用)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_input: Option<String>,
    /// 默认输出端口 (连线 source 省略 source_handle 时使用)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_output: Option<String>,
    /// RawData 控件 — 输入端口按 `src:<source>:<handle>` 动态派生, 连线时自动改写
    #[serde(default)]
    pub raw_data: bool,
}

/// 单 tab 源图 — 最近一次成功编译的节点 / 边 / 端口提示
#[derive(Debug, Clone, Default)]
pub struct TabSourceGraph {
    pub nodes: Vec<NodeDef>,
    pub edges: Vec<Edge>,
    pub hints: HashMap<String, SourceNodeHint>,
}

/// 源图存储句柄 — 按 tab 索引 (与 `AppState::graphs` 同生命周期)
pub type SourceGraphs = Arc<parking_lot::Mutex<HashMap<String, TabSourceGraph>>>;
