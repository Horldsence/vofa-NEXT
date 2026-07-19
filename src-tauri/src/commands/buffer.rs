use crate::state::AppState;
use std::time::Duration;
use tauri::{ipc::Channel, State};
use vofa_next_buffer::{RawDataBatch, WaveformWindow};
use vofa_next_core::Result;

/// 订阅波形数据 — 通过 Tauri Channel 推送窗口数据
///
/// interval_ms: 推送间隔 (毫秒), 默认 33ms (~30 FPS)
/// max_points: 单次推送的最大点数, 默认 1000
///
/// 取消方式: 前端调用 unsubscribe_waveform(channel_id) 触发 oneshot 取消信号,
/// task 在 select! 中收到信号后优雅退出, 避免向已关闭的 channel send 产生警告。
#[tauri::command]
pub async fn subscribe_waveform(
    state: State<'_, AppState>,
    on_event: Channel<WaveformWindow>,
    interval_ms: Option<u64>,
    max_points: Option<usize>,
) -> Result<()> {
    let buffer = state.buffer.clone();
    let interval = Duration::from_millis(interval_ms.unwrap_or(33));
    let max_pts = max_points.unwrap_or(1000);
    let channel_id = on_event.id();

    // 创建取消信号 channel, 存入全局 state 供 unsubscribe_waveform 使用
    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.waveform_tasks.lock().insert(channel_id, cancel_tx);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        log::info!("波形订阅已启动, channel_id={}, 间隔={}ms", channel_id, interval.as_millis());
        loop {
            tokio::select! {
                // 收到取消信号 → 优雅退出
                _ = &mut cancel_rx => {
                    log::info!("波形订阅被主动取消, channel_id={}", channel_id);
                    break;
                }
                _ = ticker.tick() => {
                    let window = {
                        let buf = buffer.lock();
                        let pts = buf.point_count().min(max_pts);
                        buf.get_recent(pts)
                    };
                    // Channel 已关闭则退出
                    if on_event.send(window).is_err() {
                        log::info!("波形订阅通道已关闭, channel_id={}", channel_id);
                        break;
                    }
                }
            }
        }
    });

    Ok(())
}

/// 同步查询: 获取最近 N 个波形点
#[tauri::command]
pub async fn get_recent_waveform(
    state: State<'_, AppState>,
    count: usize,
) -> Result<WaveformWindow> {
    let buf = state.buffer.lock();
    Ok(buf.get_recent(count))
}

/// 同步查询: 获取时间窗口内的波形
///
/// start_ms / end_ms 为相对最新时间戳的偏移 (毫秒, 负数=过去)
#[tauri::command]
pub async fn get_waveform_window(
    state: State<'_, AppState>,
    start_ms: i64,
    end_ms: i64,
) -> Result<WaveformWindow> {
    let buf = state.buffer.lock();
    Ok(buf.get_window(start_ms, end_ms))
}

/// 清空数据缓冲区
#[tauri::command]
pub async fn clear_buffer(state: State<'_, AppState>) -> Result<()> {
    state.buffer.lock().clear();
    Ok(())
}

/// 设置缓冲区通道数 (清空已有数据)
#[tauri::command]
pub async fn set_buffer_channels(state: State<'_, AppState>, count: usize) -> Result<()> {
    state.buffer.lock().set_channels(count);
    Ok(())
}

/// 获取缓冲区当前通道数和点数
#[tauri::command]
pub async fn get_buffer_info(state: State<'_, AppState>) -> Result<(usize, usize)> {
    let buf = state.buffer.lock();
    Ok((buf.channel_count(), buf.point_count()))
}

/// 设置波形缓冲区最大点数
#[tauri::command]
pub async fn set_waveform_buffer_capacity(
    state: State<'_, AppState>,
    max_points: usize,
) -> Result<()> {
    state.buffer.lock().set_max_points(max_points);
    Ok(())
}

/// 设置原始数据收集器容量 (字节)
#[tauri::command]
pub async fn set_rawdata_buffer_capacity(
    state: State<'_, AppState>,
    capacity: usize,
) -> Result<()> {
    state.raw_data_collector.lock().set_capacity(capacity);
    Ok(())
}

/// 设置 CAN 帧缓冲区最大帧数
#[tauri::command]
pub async fn set_can_buffer_capacity(
    state: State<'_, AppState>,
    capacity: usize,
) -> Result<()> {
    state.can_buffer.lock().set_max_size(capacity);
    Ok(())
}

/// 设置逻辑采样缓冲区最大采样数
#[tauri::command]
pub async fn set_logic_buffer_capacity(
    state: State<'_, AppState>,
    capacity: usize,
) -> Result<()> {
    state.logic_buffer.lock().set_max_size(capacity);
    Ok(())
}

/// 取消订阅波形 — 通过 channel_id 触发 oneshot 取消信号, 让 task 优雅退出
#[tauri::command]
pub async fn unsubscribe_waveform(
    state: State<'_, AppState>,
    channel_id: u32,
) -> Result<()> {
    if let Some(tx) = state.waveform_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

// ============ 原始数据命令 ============

/// 订阅原始数据 — 通过 Channel 周期性推送 RawDataBatch
///
/// interval_ms: 推送间隔 (毫秒), 默认 16ms (~60 FPS)
/// max_bytes: 单次推送的最大字节数, 默认 65536
#[tauri::command]
pub async fn subscribe_rawdata(
    state: State<'_, AppState>,
    on_event: Channel<RawDataBatch>,
    interval_ms: Option<u64>,
    max_bytes: Option<usize>,
) -> Result<()> {
    let collector = state.raw_data_collector.clone();
    let interval = Duration::from_millis(interval_ms.unwrap_or(16));
    let max_n = max_bytes.unwrap_or(65536);
    let channel_id = on_event.id();

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.raw_data_tasks.lock().insert(channel_id, cancel_tx);

    tokio::spawn(async move {
        crate::state::rawdata_loop(collector, on_event, interval, max_n, cancel_rx).await;
    });
    Ok(())
}

/// 取消订阅原始数据
#[tauri::command]
pub async fn unsubscribe_rawdata(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    if let Some(tx) = state.raw_data_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

/// 清空原始数据收集器
#[tauri::command]
pub async fn clear_raw_data_collector(state: State<'_, AppState>) -> Result<()> {
    state.raw_data_collector.lock().clear();
    Ok(())
}
