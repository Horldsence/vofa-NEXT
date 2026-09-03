//! 波形缓冲区 Tauri 命令 — 同步查询入口 + 命令帧打包
//!
//! 波形订阅统一走 [`display`] 的 `subscribe_data` 单一入口 (VNDP 协议);
//! 本模块仅保留缓冲区的拉取/清空/容量命令与 `compute_frame_bytes` IPC。

use app_state::AppState;
use buffer_databuffer::WaveformWindow;
use tauri::State;
use vofa_core::Result;

/// 同步查询: 获取最近 N 个波形点 (source = 数据源 Protocol 节点 id)
#[tauri::command]
pub async fn get_recent_waveform(
    state: State<'_, AppState>,
    source: String,
    count: usize,
) -> Result<WaveformWindow> {
    let buf = state.data_plane.buffer_for(&source);
    let window = buf.lock().get_recent(count);
    Ok(window)
}

/// 同步查询: 获取时间窗口内的波形
///
/// start_ms / end_ms 为相对最新时间戳的偏移 (毫秒, 负数=过去)
#[tauri::command]
pub async fn get_waveform_window(
    state: State<'_, AppState>,
    source: String,
    start_ms: i64,
    end_ms: i64,
) -> Result<WaveformWindow> {
    let buf = state.data_plane.buffer_for(&source);
    let window = buf.lock().get_window(start_ms, end_ms);
    Ok(window)
}

/// 清空数据缓冲区 (source = 数据源 Protocol 节点 id)
#[tauri::command]
pub async fn clear_buffer(state: State<'_, AppState>, source: String) -> Result<()> {
    state.data_plane.buffer_for(&source).lock().clear();
    Ok(())
}

/// 设置缓冲区通道数 (清空已有数据)
#[tauri::command]
pub async fn set_buffer_channels(
    state: State<'_, AppState>,
    source: String,
    count: usize,
) -> Result<()> {
    state
        .data_plane
        .buffer_for(&source)
        .lock()
        .set_channels(count);
    Ok(())
}

/// 获取缓冲区当前通道数和点数
#[tauri::command]
pub async fn get_buffer_info(state: State<'_, AppState>, source: String) -> Result<(usize, usize)> {
    let buf = state.data_plane.buffer_for(&source);
    let b = buf.lock();
    Ok((b.channel_count(), b.point_count()))
}

/// 设置波形缓冲区最大点数
#[tauri::command]
pub async fn set_waveform_buffer_capacity(
    state: State<'_, AppState>,
    source: String,
    max_points: usize,
) -> Result<()> {
    state
        .data_plane
        .buffer_for(&source)
        .lock()
        .set_max_points(max_points);
    Ok(())
}

/// 命令发送帧字节打包 — 后端单一权威 (`compute_frame_bytes` IPC)
///
/// `frame`: 来自前端的 `CommandFrameDto` (snake_case 序列化)
/// `inputs`: var_ref 端口的实时输入值 (按 port_name 索引, f64 表示;
///
/// 返回 `ComputedFrameDto { bytes: Vec<u8> | null, error: String | null, per_block }`。
/// 错误时 `bytes` 为 null 并附带 `块 #N: ...` 形式错误信息。
// serde IPC 契约使用 std hasher
#[allow(clippy::implicit_hasher)]
#[tauri::command]
pub async fn compute_command_frame_bytes(
    frame: crate::CommandFrameDto,
    inputs: std::collections::HashMap<String, f64>,
) -> crate::ComputedFrameDto {
    crate::compute_frame_bytes(&frame, &inputs)
}
