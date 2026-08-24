//! 图编译错误强类型 — `CompileError` (变体枚举) + `CompileReport` (受节点/边影响).
//!
//! 设计要点:
//! - **强类型变体**:每个错误有独立结构化字段, 无 `catch-all String`,
//!   经 IPC 序列化传递 (前端 `GraphCompileEvent` 直接消费)
//! - **影响范围可查询**:`affects_nodes()` / `affects_edges()` 给出受影响的节点 / 边 id
//!   列表 (去重), 供前端画布红框与 tab 角标使用
//! - **PortDomain DTO**:`node_kind::PortDomain` 是领域定义来源, 本 enum 为事件契约
//!   提供 serde-friendly 表达; 二者字段含义一一对应
//! - **后端薄壳**:该类型同时被 `node_engine::errors` 通过 `pub use` 暴露, 保持
//!   `node_engine::errors::CompileError` 的历史调用面
//!
//! 跨 IPC 错误流:
//! `node_engine::TypedGraph::build(...) -> Result<_, CompileError>`
//! → `cmd_graph::update_tab_graph` 包装为 `ConfigError::GraphCompile(Box<CompileError>)`
//! → `error::AppError::Graph(Boxed)` → JSON 序列化
//! → 前端 `lib/tauri/errorGuidance.ts` 决定用户文案

use serde::{Deserialize, Serialize};

/// 跨 IPC 端口域 DTO — 与 `node_kind::PortDomain` 含义一致, 提供 serde 派生.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PortDomain {
    F32,
    Bytes,
    String,
}

impl PortDomain {
    /// 字符串化 (诊断 / 日志)
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::Bytes => "bytes",
            Self::String => "string",
        }
    }
}

/// 图编译错误 — 强类型变体, 无 catch-all 字符串
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompileError {
    /// 数值平面循环 — `cycle` 为完整环路径 (首节点在尾部重复出现)
    #[serde(rename = "value_cycle")]
    Cycle { cycle: Vec<String> },

    /// 字节平面循环 — `cycle` 完整环路径
    #[serde(rename = "byte_cycle")]
    ByteCycle { cycle: Vec<String> },

    /// 边两端端口域不匹配
    DomainMismatch {
        edge_id: String,
        source_node: String,
        source_port: String,
        src_domain: PortDomain,
        target: String,
        target_port: String,
        tgt_domain: PortDomain,
    },

    /// 节点未找到 — 边引用的目标节点 id 不存在
    NodeNotFound { id: String },
}

impl CompileError {
    /// 涉及的节点 id (去重) — 供前端画布红框高亮定位
    pub fn affects_nodes(&self) -> Vec<String> {
        match self {
            Self::Cycle { cycle } | Self::ByteCycle { cycle } => dedup(cycle),
            Self::DomainMismatch {
                source_node, target, ..
            } => dedup(&[source_node.clone(), target.clone()]),
            Self::NodeNotFound { id } => vec![id.clone()],
        }
    }

    /// 涉及的边 id — 供前端画布红边 + hover tooltip
    pub fn affects_edges(&self) -> Vec<String> {
        match self {
            Self::DomainMismatch { edge_id, .. } => vec![edge_id.clone()],
            _ => vec![],
        }
    }

    /// 跨 IPC 的稳定错误种类 (与 `crate::Error::kind()` 一致)
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Cycle { .. } => "ValueCycle",
            Self::ByteCycle { .. } => "ByteCycle",
            Self::DomainMismatch { .. } => "DomainMismatch",
            Self::NodeNotFound { .. } => "NodeNotFound",
        }
    }

    /// 人类可读消息 — 前端可读兜底 (前端有 `errorGuidance.ts` 进一步本地化)
    pub fn message(&self) -> String {
        match self {
            Self::Cycle { cycle } => format!("数值平面检测到循环连接: {}", cycle.join(" → ")),
            Self::ByteCycle { cycle } => format!("字节平面检测到循环连接: {}", cycle.join(" → ")),
            Self::DomainMismatch {
                edge_id,
                source_node,
                source_port,
                src_domain,
                target,
                target_port,
                tgt_domain,
            } => format!(
                "边 {} 端口域不匹配: {}.{} ({}) → {}.{} ({})",
                edge_id,
                source_node,
                source_port,
                src_domain.as_str(),
                target,
                target_port,
                tgt_domain.as_str()
            ),
            Self::NodeNotFound { id } => format!("节点 {id} 不存在于图中"),
        }
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for CompileError {}

impl crate::Error for CompileError {
    fn kind(&self) -> &'static str {
        "Graph"
    }
}

impl From<CompileError> for crate::AppError {
    fn from(e: CompileError) -> Self {
        Self::Graph(Box::new(e))
    }
}

/// 编译报告 — `error` + 受影响节点/边 (供前端画布直接消费)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompileReport {
    pub error: CompileError,
    pub nodes: Vec<String>,
    pub edges: Vec<String>,
}

impl CompileReport {
    /// 从错误构造 — 自动去重填入 affected_nodes/edges
    pub fn new(error: CompileError) -> Self {
        Self {
            nodes: error.affects_nodes(),
            edges: error.affects_edges(),
            error,
        }
    }
}

impl std::fmt::Display for CompileReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}

fn dedup(items: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::with_capacity(items.len());
    let mut out = Vec::with_capacity(items.len());
    for x in items {
        if seen.insert(x.clone()) {
            out.push(x.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_extracts_nodes() {
        let e = CompileError::Cycle {
            cycle: vec!["a".into(), "b".into(), "a".into()],
        };
        assert_eq!(e.affects_nodes(), vec!["a", "b"]);
        assert!(e.affects_edges().is_empty());
        assert_eq!(e.kind(), "ValueCycle");
    }

    #[test]
    fn byte_cycle_extracts_nodes() {
        let e = CompileError::ByteCycle {
            cycle: vec!["tp".into(), "tp".into()],
        };
        assert_eq!(e.affects_nodes(), vec!["tp"]);
        assert_eq!(e.kind(), "ByteCycle");
    }

    #[test]
    fn domain_mismatch_extracts_both_endpoints_and_edge() {
        let e = CompileError::DomainMismatch {
            edge_id: "e1".into(),
            source_node: "src".into(),
            source_port: "out".into(),
            src_domain: PortDomain::Bytes,
            target: "tgt".into(),
            target_port: "in0".into(),
            tgt_domain: PortDomain::F32,
        };
        let nodes = e.affects_nodes();
        assert_eq!(nodes.len(), 2);
        assert!(nodes.contains(&"src".to_string()));
        assert!(nodes.contains(&"tgt".to_string()));
        assert_eq!(e.affects_edges(), vec!["e1"]);
    }

    #[test]
    fn node_not_found_only_one_node() {
        let e = CompileError::NodeNotFound { id: "ghost".into() };
        assert_eq!(e.affects_nodes(), vec!["ghost"]);
        assert!(e.affects_edges().is_empty());
    }

    #[test]
    fn report_fills_nodes_and_edges_from_error() {
        let err = CompileError::DomainMismatch {
            edge_id: "e1".into(),
            source_node: "src".into(),
            source_port: "out".into(),
            src_domain: PortDomain::String,
            target: "tgt".into(),
            target_port: "in".into(),
            tgt_domain: PortDomain::F32,
        };
        let r = CompileReport::new(err.clone());
        assert_eq!(r.edges, vec!["e1"]);
        assert!(r.nodes.contains(&"src".to_string()));
        assert!(r.nodes.contains(&"tgt".to_string()));
        assert_eq!(r.to_string(), err.to_string());
    }

    #[test]
    fn serde_shape_matches_event_payload() {
        let r = CompileReport::new(CompileError::DomainMismatch {
            edge_id: "e1".into(),
            source_node: "src".into(),
            source_port: "out".into(),
            src_domain: PortDomain::Bytes,
            target: "tgt".into(),
            target_port: "in0".into(),
            tgt_domain: PortDomain::F32,
        });
        let v = serde_json::to_value(&r).expect("serialize");
        assert_eq!(v["error"]["kind"], "domain_mismatch");
        assert_eq!(v["error"]["edge_id"], "e1");
        assert_eq!(v["error"]["src_domain"], "bytes");
        assert_eq!(v["edges"][0], "e1");
        assert!(v["nodes"].as_array().unwrap().len() == 2);
    }

    #[test]
    fn port_domain_str_mapping() {
        assert_eq!(PortDomain::F32.as_str(), "f32");
        assert_eq!(PortDomain::Bytes.as_str(), "bytes");
        assert_eq!(PortDomain::String.as_str(), "string");
    }
}
