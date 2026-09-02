//! 自动流水线安全上限配置。

use app_state::AppState;
use pipeline_bus::RuntimeLimits;
use tauri::State;
use vofa_core::{PipelineConfig, Result};

/// 设置流水线参数
///
#[tauri::command]
pub fn set_pipeline_config(state: State<'_, AppState>, config: PipelineConfig) -> Result<()> {
    let cfg = PipelineConfig {
        mode: config.mode,
        max_workers: config.max_workers.clamp(1, 64),
        eval_workers: config.eval_workers.clamp(1, 16),
        eval_backend: config.eval_backend,
        gpu_min_batch: config.gpu_min_batch,
        memory_budget_mb: config.memory_budget_mb.clamp(32, 4096),
        preview_fps_limit: config.preview_fps_limit.clamp(1, 120),
        preview_bandwidth_mb_per_sec: config.preview_bandwidth_mb_per_sec.clamp(1, 1024),
    };
    state.data_plane.eval.data_bus.set_limits(RuntimeLimits {
        max_workers: cfg.max_workers,
        memory_budget_mb: cfg.memory_budget_mb,
        preview_fps_limit: cfg.preview_fps_limit,
        preview_bandwidth_mb_per_sec: cfg.preview_bandwidth_mb_per_sec,
    });
    log::info!("流水线参数已更新 (clamp 后): {cfg:?}");
    *state.pipeline_config.write() = cfg;
    Ok(())
}

/// 读取当前流水线参数
#[tauri::command]
pub fn get_pipeline_config(state: State<'_, AppState>) -> Result<PipelineConfig> {
    Ok(*state.pipeline_config.read())
}
