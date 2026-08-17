use crate::state::AppState;
use std::time::Duration;
use tauri::{ipc::Channel, State};
use vofa_next_core::{
    DecodedEventBatch, DecodedEventFilter, LogicSampleBatch, LogicSampleFilter, Result,
};

// ============ 逻辑分析仪命令 ============

/// 订阅逻辑采样数据 — 统一分片流 (增量 drain + 自动并发分片)
///
/// 首次调用不传 group_id 建组 (返回组 id, 首批回溯最近 max_samples 条),
/// 后续传 group_id 加入新分片。取消: 每分片调 unsubscribe_logic_samples(channel_id)
#[tauri::command]
pub async fn subscribe_logic_samples(
    state: State<'_, AppState>,
    on_event: Channel<LogicSampleBatch>,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_samples: Option<usize>,
) -> Result<String> {
    let interval = Duration::from_millis(interval_ms.unwrap_or(100));
    let max_n = max_samples.unwrap_or(500);
    let channel_id = on_event.id();
    let buffer = state.logic_buffer.clone();

    let (source, seq, shard_idx, group_key) = crate::pipeline::stream::join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || crate::pipeline::stream::LogicStreamSource::new(buffer, max_n),
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.logic_tasks.lock().insert(channel_id, cancel_tx);

    let groups = state.stream_groups.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        crate::pipeline::stream::sharded_stream_loop(
            format!("逻辑采样分片{}", shard_idx),
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

/// 取消订阅逻辑采样
#[tauri::command]
pub async fn unsubscribe_logic_samples(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    if let Some(tx) = state.logic_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

/// 订阅带过滤条件的逻辑采样 — 统一分片流
///
/// 后端只推送匹配 filter 的采样; 游标从最旧可读位置开始, 先拉历史匹配, 之后增量。
/// 取消方式相同: unsubscribe_logic_samples(channel_id)
#[tauri::command]
pub async fn subscribe_logic_samples_filtered(
    state: State<'_, AppState>,
    on_event: Channel<LogicSampleBatch>,
    filter: LogicSampleFilter,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_samples: Option<usize>,
) -> Result<String> {
    let interval = Duration::from_millis(interval_ms.unwrap_or(100));
    let max_n = max_samples.unwrap_or(500);
    let channel_id = on_event.id();
    let buffer = state.logic_buffer.clone();

    let (source, seq, shard_idx, group_key) = crate::pipeline::stream::join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || crate::pipeline::filtered_sources::FilteredLogicStreamSource::new(buffer, filter),
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.logic_tasks.lock().insert(channel_id, cancel_tx);

    let groups = state.stream_groups.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        crate::pipeline::stream::sharded_stream_loop(
            format!("过滤逻辑采样分片{}", shard_idx),
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

/// 订阅解码事件 — 统一分片流 (增量 drain + 自动并发分片)
///
/// 首次调用不传 group_id 建组 (返回组 id, 首批回溯最近 max_events 条),
/// 后续传 group_id 加入新分片。取消: 每分片调 unsubscribe_decoded_events(channel_id)
#[tauri::command]
pub async fn subscribe_decoded_events(
    state: State<'_, AppState>,
    on_event: Channel<DecodedEventBatch>,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_events: Option<usize>,
) -> Result<String> {
    let interval = Duration::from_millis(interval_ms.unwrap_or(100));
    let max_n = max_events.unwrap_or(200);
    let channel_id = on_event.id();
    let buffer = state.decoded_buffer.clone();

    let (source, seq, shard_idx, group_key) = crate::pipeline::stream::join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || crate::pipeline::stream::DecodedStreamSource::new(buffer, max_n),
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.decoded_tasks.lock().insert(channel_id, cancel_tx);

    let groups = state.stream_groups.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        crate::pipeline::stream::sharded_stream_loop(
            format!("解码事件分片{}", shard_idx),
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

/// 取消订阅解码事件
#[tauri::command]
pub async fn unsubscribe_decoded_events(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    if let Some(tx) = state.decoded_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

/// 订阅带过滤条件的解码事件 — 统一分片流
///
/// 后端只推送匹配 filter 的事件; 游标从最旧可读位置开始, 先拉历史匹配, 之后增量。
/// 取消方式相同: unsubscribe_decoded_events(channel_id)
#[tauri::command]
pub async fn subscribe_decoded_events_filtered(
    state: State<'_, AppState>,
    on_event: Channel<DecodedEventBatch>,
    filter: DecodedEventFilter,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_events: Option<usize>,
) -> Result<String> {
    let interval = Duration::from_millis(interval_ms.unwrap_or(100));
    let max_n = max_events.unwrap_or(200);
    let channel_id = on_event.id();
    let buffer = state.decoded_buffer.clone();

    let (source, seq, shard_idx, group_key) = crate::pipeline::stream::join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || crate::pipeline::filtered_sources::FilteredDecodedStreamSource::new(buffer, filter),
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.decoded_tasks.lock().insert(channel_id, cancel_tx);

    let groups = state.stream_groups.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        crate::pipeline::stream::sharded_stream_loop(
            format!("过滤解码事件分片{}", shard_idx),
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
