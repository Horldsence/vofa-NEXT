//! 原始数据 Tauri 命令 — 缓冲区容量/清空入口
//!
//! 原始数据订阅统一走 [`display`] 的 `subscribe_data` 单一入口 (VNDP 协议);
//! 本模块仅保留各缓冲区的容量设置与收集器清空命令。

use app_state::AppState;
use tauri::State;
use vofa_core::Result;

/// 清空原始数据收集器 (source 指定的 Transport 源 / None = 全部源;
/// 各 FrameDecoder 节点旁路收集器总是同时清空)
#[tauri::command]
pub async fn clear_raw_data_collector(
    state: State<'_, AppState>,
    source: Option<String>,
) -> Result<()> {
    match source {
        Some(s) => state.data_plane.raw_collector_for(&s).lock().clear(),
        None => {
            for c in state.data_plane.raw_collectors.lock().values() {
                c.lock().clear();
            }
        }
    }
    for collector in state.decoder_raw_collectors.lock().values() {
        collector.lock().clear();
    }
    Ok(())
}

/// 设置原始数据收集器容量 (字节, source = Transport 节点 id)
#[tauri::command]
pub async fn set_rawdata_buffer_capacity(
    state: State<'_, AppState>,
    source: String,
    capacity: usize,
) -> Result<()> {
    state
        .data_plane
        .raw_collector_for(&source)
        .lock()
        .set_capacity(capacity);
    Ok(())
}

/// 设置 CAN 帧缓冲区最大帧数
#[tauri::command]
pub async fn set_can_buffer_capacity(state: State<'_, AppState>, capacity: usize) -> Result<()> {
    state.can_buffer.lock().set_max_size(capacity);
    Ok(())
}

/// 设置逻辑采样缓冲区最大采样数
#[tauri::command]
pub async fn set_logic_buffer_capacity(state: State<'_, AppState>, capacity: usize) -> Result<()> {
    state.logic_buffer.lock().set_max_size(capacity);
    Ok(())
}
