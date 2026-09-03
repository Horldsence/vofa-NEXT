//! 节点规格（spec）— IPC 节点定义 `NodeDef` + 端口命名派生助手.
//!
//! 与前端 `src/lib/utils/nodeDef.ts` 的类型镜像对齐:
//! `{ id: string, tab_id: string, kind: NodeKind }`.

use serde::{Deserialize, Serialize};

use crate::kind::NodeKind;

/// 节点定义 — 通过 IPC 从前端同步到后端
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDef {
    pub id: String,
    pub tab_id: String,
    pub kind: NodeKind,
}

impl NodeDef {
    /// 构造一个节点定义（前端镜像构造常用入口）
    pub const fn new(id: String, tab_id: String, kind: NodeKind) -> Self {
        Self { id, tab_id, kind }
    }

    /// 节点 id 的字符串视图（trait `NodeSpec::id()` 的兼容入口）
    pub fn id_str(&self) -> &str {
        &self.id
    }

    /// tab id 的字符串视图
    pub fn tab_id_str(&self) -> &str {
        &self.tab_id
    }
}

/// 解析 ProtocolSource 的输出端口名列表（编译/求值共用）
///
/// `port_names` 给定且非空时用命名端口（越界/空名回退 `"ch{i}"`），否则缺省 `"ch0".."chN"`
pub fn protocol_source_port_names(port_names: Option<&[String]>, channels: usize) -> Vec<String> {
    (0..channels)
        .map(|i| {
            port_names
                .and_then(|ps| ps.get(i))
                .filter(|p| !p.is_empty())
                .cloned()
                .unwrap_or_else(|| format!("ch{i}"))
        })
        .collect()
}
