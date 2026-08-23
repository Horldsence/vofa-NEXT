//! 图编译错误 — 强类型变体, 完整环路径诊断, 无 `String` catch-all

use node_kind::PortDomain;

/// 图编译错误
#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("节点 {id} 不存在于图中")]
    NodeNotFound { id: String },

    /// 数值平面循环 — cycle 为完整环路径 (首节点在尾部重复出现),
    /// 如 `[a, b, a]` 表示 a → b → a
    #[error("数值平面检测到循环连接: {}", .cycle.join(" → "))]
    Cycle { cycle: Vec<String> },

    /// 字节平面循环 — 完整环路径 (同 [`Cycle`](Self::Cycle))
    #[error("字节平面检测到循环连接: {}", .cycle.join(" → "))]
    ByteCycle { cycle: Vec<String> },

    #[error("边 {edge_id} 端口域不匹配: {source_node}.{source_port} ({src_domain:?}) → {target}.{target_port} ({tgt_domain:?})")]
    DomainMismatch {
        edge_id: String,
        source_node: String,
        source_port: String,
        src_domain: PortDomain,
        target: String,
        target_port: String,
        tgt_domain: PortDomain,
    },
}

impl error::Error for CompileError {
    fn kind(&self) -> &'static str {
        "Graph"
    }
}

impl From<CompileError> for error::AppError {
    fn from(e: CompileError) -> Self {
        Self::Graph(Box::new(e))
    }
}
