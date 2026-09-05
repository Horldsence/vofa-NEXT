//! 总线公共类型 — Topic 标识 / 样本 / 批次 / 运行时配置与健康

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 一个数值端口的稳定标识。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TopicKey {
    pub source_node_id: String,
    pub source_handle: String,
}

impl TopicKey {
    #[must_use]
    pub fn new(source_node_id: impl Into<String>, source_handle: impl Into<String>) -> Self {
        Self {
            source_node_id: source_node_id.into(),
            source_handle: source_handle.into(),
        }
    }
}

/// Topic 当前数据状态。状态与样本值正交，零值不会被误判为断开。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SampleStatus {
    Waiting,
    Live,
    Disconnected,
    ChannelOutOfRange { requested: usize, available: usize },
    Overrun { lost_samples: u64 },
}

/// 单个有效样本。无效/缺失数据不会构造此类型。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub sequence: u64,
    pub timestamp_us: u64,
    pub value: f64,
}

/// Actor 向订阅者发布的有序批次。
#[derive(Debug, Clone)]
pub struct SampleBatch {
    pub topic: TopicKey,
    pub sequence: u64,
    pub samples: Arc<[Sample]>,
    pub status: SampleStatus,
    pub preview_skipped: u64,
    pub retention_evicted: u64,
    pub ingress_dropped: u64,
}

/// 自动运行模式的安全上限。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeLimits {
    pub max_workers: usize,
    pub memory_budget_mb: usize,
    pub preview_fps_limit: u32,
    pub preview_bandwidth_mb_per_sec: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_workers: 8,
            memory_budget_mb: 256,
            preview_fps_limit: 60,
            preview_bandwidth_mb_per_sec: 8,
        }
    }
}

/// 可从前端查询的累计健康信息。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeHealth {
    pub active_topics: u64,
    pub published_samples: u64,
    pub preview_skipped: u64,
    pub retention_evicted: u64,
    pub ingress_dropped: u64,
    pub last_ack_sequence: u64,
    pub recommended_interval_ms: u64,
}
