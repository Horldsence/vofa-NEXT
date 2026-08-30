//! 自动数据管道的安全上限。具体 worker、合批和推送速率由运行时控制器决定。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PipelineMode {
    #[default]
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    pub mode: PipelineMode,
    pub max_workers: usize,
    pub memory_budget_mb: usize,
    pub preview_fps_limit: u32,
    pub preview_bandwidth_mb_per_sec: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            mode: PipelineMode::Auto,
            max_workers: 8,
            memory_budget_mb: 256,
            preview_fps_limit: 60,
            preview_bandwidth_mb_per_sec: 8,
        }
    }
}
