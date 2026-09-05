//! 自动流水线安全上限配置。

use app_state::AppState;
use data_bus::RuntimeLimits;
use tauri::State;
use vofa_core::{PipelineConfig, Result};

/// 配置钳制 (纯函数) — 前端可提交任意值, 后端统一收敛到安全区间:
/// max_workers 1-64 / eval_workers 1-16 / memory 32-4096 MB / fps 1-120 / bandwidth 1-1024 MB/s
fn sanitize_pipeline_config(config: &PipelineConfig) -> PipelineConfig {
    PipelineConfig {
        mode: config.mode,
        max_workers: config.max_workers.clamp(1, 64),
        eval_workers: config.eval_workers.clamp(1, 16),
        eval_simd: config.eval_simd,
        memory_budget_mb: config.memory_budget_mb.clamp(32, 4096),
        preview_fps_limit: config.preview_fps_limit.clamp(1, 120),
        preview_bandwidth_mb_per_sec: config.preview_bandwidth_mb_per_sec.clamp(1, 1024),
    }
}

/// 设置流水线参数
///
#[tauri::command]
pub fn set_pipeline_config(state: State<'_, AppState>, config: PipelineConfig) -> Result<()> {
    let cfg = sanitize_pipeline_config(&config);
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

#[cfg(test)]
mod tests {
    use super::*;
    use vofa_core::PipelineMode;

    fn config() -> PipelineConfig {
        PipelineConfig {
            mode: PipelineMode::Auto,
            max_workers: 8,
            eval_workers: 4,
            eval_simd: true,
            memory_budget_mb: 256,
            preview_fps_limit: 60,
            preview_bandwidth_mb_per_sec: 8,
        }
    }

    #[test]
    fn in_range_config_passes_through() {
        let cfg = sanitize_pipeline_config(&config());
        assert_eq!(cfg.max_workers, 8);
        assert_eq!(cfg.eval_workers, 4);
        assert_eq!(cfg.memory_budget_mb, 256);
        assert_eq!(cfg.preview_fps_limit, 60);
        assert_eq!(cfg.preview_bandwidth_mb_per_sec, 8);
        assert_eq!(cfg.mode, PipelineMode::Auto);
        assert!(cfg.eval_simd);
    }

    #[test]
    fn zero_values_clamp_to_lower_bounds() {
        let mut cfg = config();
        cfg.max_workers = 0;
        cfg.eval_workers = 0;
        cfg.memory_budget_mb = 0;
        cfg.preview_fps_limit = 0;
        cfg.preview_bandwidth_mb_per_sec = 0;
        let cfg = sanitize_pipeline_config(&cfg);
        assert_eq!(cfg.max_workers, 1);
        assert_eq!(cfg.eval_workers, 1);
        assert_eq!(cfg.memory_budget_mb, 32);
        assert_eq!(cfg.preview_fps_limit, 1);
        assert_eq!(cfg.preview_bandwidth_mb_per_sec, 1);
    }

    #[test]
    fn oversized_values_clamp_to_upper_bounds() {
        let mut cfg = config();
        cfg.max_workers = 1_000;
        cfg.eval_workers = 1_000;
        cfg.memory_budget_mb = 1_000_000;
        cfg.preview_fps_limit = 1_000;
        cfg.preview_bandwidth_mb_per_sec = 1_000_000;
        let cfg = sanitize_pipeline_config(&cfg);
        assert_eq!(cfg.max_workers, 64);
        assert_eq!(cfg.eval_workers, 16);
        assert_eq!(cfg.memory_budget_mb, 4096);
        assert_eq!(cfg.preview_fps_limit, 120);
        assert_eq!(cfg.preview_bandwidth_mb_per_sec, 1024);
    }
}
