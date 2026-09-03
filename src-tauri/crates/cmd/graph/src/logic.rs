//! 逻辑分析仪 / 解码事件 Tauri 命令 — 同步查询入口
//!
//! 订阅统一走 [`display`] 的 `subscribe_data` 单一入口 (VNDP 协议);
//! 本模块仅保留缓冲区的拉取/清空/容量查询命令。

use app_state::AppState;
use logic_types::{DecodedEventBatch, LogicSampleBatch};
use tauri::State;
use vofa_core::Result;

// ============ 逻辑分析仪命令 ============

/// 同步查询: 获取最近 N 个逻辑采样
#[tauri::command]
pub async fn get_recent_logic_samples(
    state: State<'_, AppState>,
    count: usize,
) -> Result<LogicSampleBatch> {
    let samples = state.logic_buffer.lock().get_recent(count);
    Ok(LogicSampleBatch { seq: 0, samples })
}

/// 清空逻辑采样缓冲区
#[tauri::command]
pub async fn clear_logic_buffer(state: State<'_, AppState>) -> Result<()> {
    state.logic_buffer.lock().clear();
    Ok(())
}

/// 获取逻辑采样缓冲区当前数量
#[tauri::command]
pub async fn get_logic_buffer_info(state: State<'_, AppState>) -> Result<usize> {
    Ok(state.logic_buffer.lock().len())
}

/// 同步查询: 获取最近 N 个解码事件
#[tauri::command]
pub async fn get_recent_decoded_events(
    state: State<'_, AppState>,
    count: usize,
) -> Result<DecodedEventBatch> {
    let events = state.decoded_buffer.lock().get_recent(count);
    Ok(DecodedEventBatch { seq: 0, events })
}

/// 清空解码事件缓冲区
#[tauri::command]
pub async fn clear_decoded_buffer(state: State<'_, AppState>) -> Result<()> {
    state.decoded_buffer.lock().clear();
    Ok(())
}

/// 获取解码事件缓冲区当前数量
#[tauri::command]
pub async fn get_decoded_buffer_info(state: State<'_, AppState>) -> Result<usize> {
    Ok(state.decoded_buffer.lock().len())
}
