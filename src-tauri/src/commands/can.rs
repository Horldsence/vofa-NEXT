use crate::state::AppState;
use std::time::Duration;
use tauri::{ipc::Channel, State};
use vofa_next_core::{CanFrame, CanFrameBatch, CandleDeviceInfo, Result};

// ============ CAN 帧相关 ============

/// 发送 CAN 帧
///
/// 通过当前协议引擎的 encode_can 编码为字节, 再通过传输层发送。
/// 若当前协议不是 CAN 协议 (encode_can 返回空), 直接返回 Ok。
#[tauri::command]
pub async fn send_can_frame(state: State<'_, AppState>, frame: CanFrame) -> Result<()> {
    let data = state.protocol.lock().encode_can(&frame);
    if data.is_empty() {
        return Ok(()); // 非 CAN 协议, 忽略
    }
    let manager = state.transport.lock().await;
    manager.send(&data).await
}

/// 订阅 CAN 帧推送 — 通过 Channel 定期推送最近 N 帧
///
/// - interval_ms: 推送间隔 (默认 100ms)
/// - max_frames: 单次推送最大帧数 (默认 500)
///
/// 取消方式: 前端调用 unsubscribe_can_frames(channel_id)
#[tauri::command]
pub async fn subscribe_can_frames(
    state: State<'_, AppState>,
    on_event: Channel<CanFrameBatch>,
    interval_ms: Option<u64>,
    max_frames: Option<usize>,
) -> Result<()> {
    let buffer = state.can_buffer.clone();
    let interval = Duration::from_millis(interval_ms.unwrap_or(100));
    let max_n = max_frames.unwrap_or(500);
    let channel_id = on_event.id();

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.can_tasks.lock().insert(channel_id, cancel_tx);

    tokio::spawn(async move {
        crate::state::can_frames_loop(buffer, on_event, interval, max_n, cancel_rx).await;
    });
    Ok(())
}

/// 取消订阅 CAN 帧
#[tauri::command]
pub async fn unsubscribe_can_frames(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    if let Some(tx) = state.can_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

/// 同步查询: 获取最近 N 个 CAN 帧
#[tauri::command]
pub async fn get_recent_can_frames(
    state: State<'_, AppState>,
    count: usize,
) -> Result<Vec<CanFrame>> {
    Ok(state.can_buffer.lock().get_recent(count))
}

/// 清空 CAN 帧缓冲区
#[tauri::command]
pub async fn clear_can_buffer(state: State<'_, AppState>) -> Result<()> {
    state.can_buffer.lock().clear();
    Ok(())
}

/// 获取 CAN 缓冲区当前帧数
#[tauri::command]
pub async fn get_can_buffer_info(state: State<'_, AppState>) -> Result<usize> {
    Ok(state.can_buffer.lock().len())
}

/// 列出所有 candleLight 设备
#[tauri::command]
pub async fn list_candle_devices() -> Result<Vec<CandleDeviceInfo>> {
    vofa_next_transport::candle::list_devices()
}
