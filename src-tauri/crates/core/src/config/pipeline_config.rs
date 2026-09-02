//! 自动数据管道的安全上限。具体 worker、合批和推送速率由运行时控制器决定。

use serde::{Deserialize, Serialize};

/// 数值平面评估后端
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
    /// 数值平面评估 worker 数 — 1 = 串行 (默认, 行为与旧版一致);
    /// ≥2 启用图内路径 (评估单元) 分块 fork-join 并行评估
    pub eval_workers: usize,
    /// fork-join 并行路径内 Math 单元的 SciRS2 SIMD 批量求值开关
    /// (仅 eval_workers ≥ 2 生效; 结果与标量路径逐位一致)
    pub eval_simd: bool,
    pub memory_budget_mb: usize,
    pub preview_fps_limit: u32,
    pub preview_bandwidth_mb_per_sec: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            mode: PipelineMode::Auto,
            max_workers: 8,
            eval_workers: 1,
            eval_simd: true,
            memory_budget_mb: 256,
            preview_fps_limit: 60,
            preview_bandwidth_mb_per_sec: 8,
        }
    }
}
