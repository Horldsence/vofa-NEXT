use crate::state::AppState;
use std::time::Duration;
use tauri::{ipc::Channel, State};
use vofa_next_core::{DecodedEventBatch, LogicSampleBatch, Result};

// ============ 逻辑分析仪命令 ============

/// 订阅逻辑采样数据 — 通过 Tauri Channel 周期性推送
#[tauri::command]
pub async fn subscribe_logic_samples(
    state: State<'_, AppState>,
    on_event: Channel<LogicSampleBatch>,
    interval_ms: Option<u64>,
    max_samples: Option<usize>,
) -> Result<()> {
    let logic_buffer = state.logic_buffer.clone();
    let interval = Duration::from_millis(interval_ms.unwrap_or(100));
    let max_n = max_samples.unwrap_or(500);
    let channel_id = on_event.id();

    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.logic_tasks.lock().insert(channel_id, cancel_tx);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        log::info!("逻辑采样订阅已启动, channel_id={}", channel_id);
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    log::info!("逻辑采样订阅被取消, channel_id={}", channel_id);
                    break;
                }
                _ = ticker.tick() => {
                    let samples = {
                        let buf = logic_buffer.lock();
                        buf.get_recent(max_n)
                    };
                    if samples.is_empty() {
                        continue;
                    }
                    if on_event.send(LogicSampleBatch { samples }).is_err() {
                        break;
                    }
                }
            }
        }
    });

    Ok(())
}

/// 取消订阅逻辑采样
#[tauri::command]
pub async fn unsubscribe_logic_samples(
    state: State<'_, AppState>,
    channel_id: u32,
) -> Result<()> {
    if let Some(tx) = state.logic_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

/// 同步查询: 获取最近 N 个逻辑采样
#[tauri::command]
pub async fn get_recent_logic_samples(
    state: State<'_, AppState>,
    count: usize,
) -> Result<LogicSampleBatch> {
    let samples = state.logic_buffer.lock().get_recent(count);
    Ok(LogicSampleBatch { samples })
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

/// 订阅解码事件 — 通过 Tauri Channel 周期性推送
#[tauri::command]
pub async fn subscribe_decoded_events(
    state: State<'_, AppState>,
    on_event: Channel<DecodedEventBatch>,
    interval_ms: Option<u64>,
    max_events: Option<usize>,
) -> Result<()> {
    let decoded_buffer = state.decoded_buffer.clone();
    let interval = Duration::from_millis(interval_ms.unwrap_or(100));
    let max_n = max_events.unwrap_or(200);
    let channel_id = on_event.id();

    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.decoded_tasks.lock().insert(channel_id, cancel_tx);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        log::info!("解码事件订阅已启动, channel_id={}", channel_id);
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    log::info!("解码事件订阅被取消, channel_id={}", channel_id);
                    break;
                }
                _ = ticker.tick() => {
                    let events = {
                        let buf = decoded_buffer.lock();
                        buf.get_recent(max_n)
                    };
                    if events.is_empty() {
                        continue;
                    }
                    if on_event.send(DecodedEventBatch { events }).is_err() {
                        break;
                    }
                }
            }
        }
    });

    Ok(())
}

/// 取消订阅解码事件
#[tauri::command]
pub async fn unsubscribe_decoded_events(
    state: State<'_, AppState>,
    channel_id: u32,
) -> Result<()> {
    if let Some(tx) = state.decoded_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

/// 同步查询: 获取最近 N 个解码事件
#[tauri::command]
pub async fn get_recent_decoded_events(
    state: State<'_, AppState>,
    count: usize,
) -> Result<DecodedEventBatch> {
    let events = state.decoded_buffer.lock().get_recent(count);
    Ok(DecodedEventBatch { events })
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
