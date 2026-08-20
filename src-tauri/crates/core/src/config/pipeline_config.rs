//! 流水线参数 — 数据通路 (合批 / 并行解析 / 流分片 / 通道容量) 的可调配置
//!
//! 前后端契约: serde snake_case 字段名, 缺省字段回退 [`PipelineConfig::default`]
//! (即下方括号内数值)。合法性 clamp 在 set_pipeline_config 命令侧执行。

use serde::{Deserialize, Serialize};

/// 流水线可调参数集合 — 全部含 `#[serde(default)]`,前端可省略任何字段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    /// feed_task 合批上限: 单次最多合并的消息条数 (64)
    pub coalesce_max_msgs: usize,
    /// feed_task 合批上限: 单次合并的字节数上限, 单位 KiB (256)
    pub coalesce_max_bytes_kb: usize,
    /// feed 段最大并行 worker 数 (4)
    pub max_feed_workers: usize,
    /// 并行升级单位: parse mpsc 每 N 批积压升一级 (8)
    pub feed_parallel_unit: usize,
    /// 每 worker 至少摊到的字节数, 单位 KiB (32)
    pub min_worker_bytes_kb: usize,
    /// 流订阅组最大分片数 (4)
    pub max_stream_shards: usize,
    /// data_loop → feed_task 的 mpsc 通道容量 (256)
    pub parse_channel_cap: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            coalesce_max_msgs: 64,
            coalesce_max_bytes_kb: 256,
            max_feed_workers: 4,
            feed_parallel_unit: 8,
            min_worker_bytes_kb: 32,
            max_stream_shards: 4,
            parse_channel_cap: 256,
        }
    }
}