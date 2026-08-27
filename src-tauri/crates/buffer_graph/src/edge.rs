//! `Edge` 节点连接边

use serde::{Deserialize, Serialize};

/// 节点连接边
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub source_handle: String,
    pub target: String,
    pub target_handle: String,
}
