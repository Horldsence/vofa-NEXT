use app_state::AppState;
use pipeline_stream::{sharded_stream_loop, join_or_create_group, leave_group, WaveformSource};
use std::time::Duration;
use tauri::{ipc::Channel, State};
use buffer_databuffer::WaveformWindow;
use vofa_core::Result;

/// 订阅波形数据 — 统一分片流 (快照语义 + 自动并发分片)
///
/// 波形是唯一快照替换流: version 变化即推送最新窗口, 前端按 "最新 seq 胜出" 处理乱序。
/// 首次调用不传 group_id 建组 (返回组 id), 后续传 group_id 加入新分片
/// (落后 ≥ 200 帧未推送时自动激活)。
///
/// source: 数据源节点 id (Protocol 节点; 每源一个 DataBuffer 实例)
/// interval_ms: 推送间隔 (毫秒), 默认 33ms (~30 FPS)
/// max_points: 单次推送的最大点数, 默认 1000
///
/// 取消方式: 前端对每个分片调用 unsubscribe_waveform(channel_id)
#[tauri::command]
pub async fn subscribe_waveform(
    state: State<'_, AppState>,
    source: String,
    on_event: Channel<WaveformWindow>,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_points: Option<usize>,
) -> Result<String> {
    let interval = Duration::from_millis(interval_ms.unwrap_or(33));
    let max_pts = max_points.unwrap_or(1000);
    let channel_id = on_event.id();
    let buffer = state.data_plane.buffer_for(&source);

    let (source, seq, shard_idx, group_key) = join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || WaveformSource::new(buffer),
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.waveform_tasks.lock().insert(channel_id, cancel_tx);

    let groups = state.stream_groups.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        sharded_stream_loop(
            format!("波形分片{shard_idx}"),
            source,
            on_event,
            shard_idx,
            seq,
            interval,
            max_pts,
            cancel_rx,
        )
        .await;
        leave_group(&groups, &exit_key);
    });

    Ok(group_key)
}

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

/// 取消订阅波形 — 通过 channel_id 触发 oneshot 取消信号, 让 task 优雅退出
#[tauri::command]
pub async fn unsubscribe_waveform(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    if let Some(tx) = state.waveform_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}
