//! 源图存储 — 连线拓扑 + widget 配置模型的后端权威 (仅存类型, op 逻辑在
//! `graph::source_graph`)
//!
//! 前端整图提交 (`update_tab_graph`) 与后端拓扑 op (`connect_edge` / `disconnect_edge`)
//! 共同维护:apply 编译成功后写入, 编译失败不落盘 (旧图保留)。
//! widget 配置记录 ([`crate::WidgetRecord`]) 与画布位置 (workspace 模块) 随同一
//! 提交路径更新;端口提示 [`SourceNodeHint`] 随 sync 附带, 供后端 op 解析
//! 默认 handle 与 RawData `src:` 端口改写 (后端无法枚举 Sink 端口)。

use crate::workspace::WidgetRecord;
use buffer_graph::Edge;
use kind::NodeDef;
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

/// 单 tab 源图 — 最近一次成功编译的节点 / 边 / 端口提示 / widget 配置记录
#[derive(Debug, Clone, Default)]
pub struct TabSourceGraph {
    pub nodes: Vec<NodeDef>,
    pub edges: Vec<Edge>,
    pub hints: HashMap<String, SourceNodeHint>,
    /// 该 tab 的 widget 配置记录 (与 `nodes` 中同 id 的 NodeDef 一一对应;
    /// 整图提交携带时整体替换, 拓扑 op 不携带时保留)
    pub widgets: Vec<WidgetRecord>,
}

/// 源图存储句柄 — 按 tab 索引 (与 `AppState::graphs` 同生命周期)
pub type SourceGraphs = Arc<parking_lot::Mutex<HashMap<String, TabSourceGraph>>>;
