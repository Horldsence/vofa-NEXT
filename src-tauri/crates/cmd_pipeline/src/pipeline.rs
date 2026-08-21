//! 流水线参数配置命令 — 合批 / 并行解析 / 流分片 / 通道容量
//!
//! 配置存于 `AppState.pipeline_config` (RwLock<PipelineConfig>), 写入后下一轮
//! 数据处理即生效 (feed_task 每批读快照; parse mpsc 容量在建通道时读一次)。

use app_state::AppState;
use tauri::State;
use vofa_core::{PipelineConfig, Result};

/// 设置流水线参数
///
/// 字段合法性 clamp:
/// - max_feed_workers 1..=8, max_stream_shards 1..=8
/// - parse_channel_cap 16..=4096, coalesce_max_bytes_kb 16..=4096
/// - coalesce_max_msgs 1..=1024, min_worker_bytes_kb 4..=1024, feed_parallel_unit 1..=256
#[tauri::command]
pub fn set_pipeline_config(state: State<'_, AppState>, config: PipelineConfig) -> Result<()> {
    let cfg = PipelineConfig {
        coalesce_max_msgs: config.coalesce_max_msgs.clamp(1, 1024),
        coalesce_max_bytes_kb: config.coalesce_max_bytes_kb.clamp(16, 4096),
        max_feed_workers: config.max_feed_workers.clamp(1, 8),
        feed_parallel_unit: config.feed_parallel_unit.clamp(1, 256),
        min_worker_bytes_kb: config.min_worker_bytes_kb.clamp(4, 1024),
        max_stream_shards: config.max_stream_shards.clamp(1, 8),
        parse_channel_cap: config.parse_channel_cap.clamp(16, 4096),
    };
    log::info!("流水线参数已更新 (clamp 后): {cfg:?}");
    *state.pipeline_config.write() = cfg;
    Ok(())
}

/// 读取当前流水线参数
#[tauri::command]
pub fn get_pipeline_config(state: State<'_, AppState>) -> Result<PipelineConfig> {
    Ok(*state.pipeline_config.read())
}
