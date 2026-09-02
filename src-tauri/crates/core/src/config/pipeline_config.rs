//! 自动数据管道的安全上限。具体 worker、合批和推送速率由运行时控制器决定。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PipelineMode {
    #[default]
    Auto,
}

/// 数值平面评估后端
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvalBackend {
    /// 自动: 有可用 GPU 适配器 + 存在 GPU 资格单元 + 批帧数 ≥ gpu_min_batch
    /// 时走 wgpu 卸载, 否则 CPU (串行/分块并行)
    #[default]
    Auto,
    /// 强制 CPU (eval_workers = 1 串行, ≥2 单元分块并行)
    Cpu,
    /// 强制 GPU (无适配器/资格单元时自动回退 CPU, 会话失败后本版本内禁用)
    Gpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    pub mode: PipelineMode,
    pub max_workers: usize,
    /// 数值平面评估 worker 数 — 1 = 串行 (默认, 行为与旧版一致);
    /// ≥2 启用图内路径 (评估单元) 分块 fork-join 并行评估
    pub eval_workers: usize,
    /// 数值平面评估后端选择 (见 [`EvalBackend`])
    pub eval_backend: EvalBackend,
    /// 自动模式下走 GPU 的最小批帧数 (小批次上传/回传开销大于收益)
    pub gpu_min_batch: usize,
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
            eval_backend: EvalBackend::Auto,
            gpu_min_batch: 1024,
            memory_budget_mb: 256,
            preview_fps_limit: 60,
            preview_bandwidth_mb_per_sec: 8,
        }
    }
}
