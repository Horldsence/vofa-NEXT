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
    /// 数值平面评估 worker 数 — ≥2 启用图内路径 (评估单元) 分块 fork-join
    /// 并行评估 (与串行路径逐位一致); 默认按 CPU 自动扩 (min(cores, 8)),
    /// 高采样率下全样本严格求值需要多 worker 才能跟上摄入
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
        // eval_workers 默认按 CPU 自动扩: 已保存的显式配置 (如 1 = 串行)
        // 经 serde 反序列化不受默认值影响
        let cores = std::thread::available_parallelism()
            .map_or(1, std::num::NonZero::get)
            .min(8);
        Self {
            mode: PipelineMode::Auto,
            max_workers: 8,
            eval_workers: cores.max(1),
            eval_simd: true,
            memory_budget_mb: 256,
            preview_fps_limit: 60,
            preview_bandwidth_mb_per_sec: 8,
        }
    }
}
