//! 配置与状态错误 — 涵盖节点缺失、流订阅组、图编译、URL 解析等。

use thiserror::Error;

use crate::{Boxed, Error};

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("节点 {node_id} 不存在")]
    NodeNotFound { node_id: String },

    #[error("协议节点不存在: {node_id}")]
    ProtocolNodeNotFound { node_id: String },

    #[error("全局字节平面编译失败: {0}")]
    BytePlanCompile(Boxed),

    #[error("图编译失败: {0}")]
    GraphCompile(Boxed),

    /// 前端整图提交的基线版本过期 (期间有其他写入方推进了图版本)。
    /// 携带后端当前版本, 前端据此拉取权威源图、合并本地态后重试。
    #[error("图版本冲突: 基线过期 (后端当前 v{current})")]
    GraphVersionConflict { current: u64 },

    /// 拓扑 op 未命中任何连线 (按 edge_id 或 source/target 组合查找)。
    #[error("未找到匹配的连线")]
    GraphEdgeNotFound,

    /// 拓扑 op 无法推断默认端口 (widget 端口表在前端, 需调用方显式指定 handle)。
    #[error("节点 {node_id} 无法推断默认{direction}端口, 请显式指定 handle (参考 get_workspace 端口表)")]
    GraphPortUnresolved { node_id: String, direction: &'static str },

    #[error("流订阅组不存在: {key}")]
    StreamGroupNotFound { key: String },

    #[error("流订阅组 {key} 已满 ({max} 分片)")]
    StreamGroupFull { key: String, max: usize },

    #[error("流订阅组类型不匹配: {key}")]
    StreamGroupTypeMismatch { key: String },

    #[error("无法确定下载目录: {0}")]
    DownloadDir(#[source] std::io::Error),

    #[error("Auto 绑定需要指定 protocol_node")]
    AutoBindingMissingProtocolNode,

    #[error("URL {url} 解析失败: {source}")]
    UrlParse {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Mutex 中毒: {0}")]
    MutexPoisoned(String),
}

impl Error for ConfigError {
    fn kind(&self) -> &'static str {
        "Config"
    }
}
