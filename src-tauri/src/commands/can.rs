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

/// 订阅 CAN 帧推送 — 统一分片流 (增量 drain + 自动并发分片)
///
/// - 首次调用不传 `group_id`: 创建订阅组 (本通道为 shard 0), 返回组 id,
///   首个批次回溯最近 max_frames 条历史, 之后严格增量推送
/// - 后续调用传入 `group_id`: 作为新 shard 加入 (最多 MAX_STREAM_SHARDS 个)
///
/// - interval_ms: 推送间隔 (默认 100ms, 有数据时自动提速到 16ms)
/// - max_frames: 单次推送最小批量 (默认 500, 随积压自适应放大)
///
/// 取消方式: 前端对每个分片调用 unsubscribe_can_frames(channel_id)
#[tauri::command]
pub async fn subscribe_can_frames(
    state: State<'_, AppState>,
    on_event: Channel<CanFrameBatch>,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_frames: Option<usize>,
) -> Result<String> {
    let interval = Duration::from_millis(interval_ms.unwrap_or(100));
    let max_n = max_frames.unwrap_or(500);
    let channel_id = on_event.id();
    let buffer = state.can_buffer.clone();

    let (source, seq, shard_idx, group_key) = crate::pipeline::stream::join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || crate::pipeline::stream::CanStreamSource::new(buffer, max_n),
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.can_tasks.lock().insert(channel_id, cancel_tx);

    let groups = state.stream_groups.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        crate::pipeline::stream::sharded_stream_loop(
            format!("CAN 帧分片{}", shard_idx),
            source,
            on_event,
            shard_idx,
            seq,
            interval,
            max_n,
            cancel_rx,
        )
        .await;
        crate::pipeline::stream::leave_group(&groups, &exit_key);
    });
    Ok(group_key)
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
