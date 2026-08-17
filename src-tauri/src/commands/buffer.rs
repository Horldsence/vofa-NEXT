use crate::state::AppState;
use std::time::Duration;
use tauri::{ipc::Channel, State};
use vofa_next_buffer::{DirectionFilter, RawDataBatch, WaveformWindow};
use vofa_next_core::Result;

/// 订阅波形数据 — 统一分片流 (快照语义 + 自动并发分片)
///
/// 波形是唯一快照替换流: version 变化即推送最新窗口, 前端按 "最新 seq 胜出" 处理乱序。
/// 首次调用不传 group_id 建组 (返回组 id), 后续传 group_id 加入新分片
/// (落后 ≥ 200 帧未推送时自动激活)。
///
/// interval_ms: 推送间隔 (毫秒), 默认 33ms (~30 FPS)
/// max_points: 单次推送的最大点数, 默认 1000
///
/// 取消方式: 前端对每个分片调用 unsubscribe_waveform(channel_id)
#[tauri::command]
pub async fn subscribe_waveform(
    state: State<'_, AppState>,
    on_event: Channel<WaveformWindow>,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_points: Option<usize>,
) -> Result<String> {
    let interval = Duration::from_millis(interval_ms.unwrap_or(33));
    let max_pts = max_points.unwrap_or(1000);
    let channel_id = on_event.id();
    let buffer = state.buffer.clone();

    let (source, seq, shard_idx, group_key) = crate::pipeline::stream::join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || crate::pipeline::stream::WaveformSource::new(buffer),
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.waveform_tasks.lock().insert(channel_id, cancel_tx);

    let groups = state.stream_groups.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        crate::pipeline::stream::sharded_stream_loop(
            format!("波形分片{}", shard_idx),
            source,
            on_event,
            shard_idx,
            seq,
            interval,
            max_pts,
            cancel_rx,
        )
        .await;
        crate::pipeline::stream::leave_group(&groups, &exit_key);
    });

    Ok(group_key)
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

/// 取消订阅波形 — 通过 channel_id 触发 oneshot 取消信号, 让 task 优雅退出
#[tauri::command]
pub async fn unsubscribe_waveform(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    if let Some(tx) = state.waveform_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

// ============ 原始数据命令 ============

/// 订阅原始数据 — 统一分片流 (增量 drain + 自动并发分片)
///
/// - 首次调用不传 `group_id`: 创建订阅组 (本通道为 shard 0), 返回组 id
/// - 后续调用传入 `group_id`: 作为新 shard 加入 (最多 MAX_STREAM_SHARDS 个)
///
/// 分片按积压自动激活/休眠 (shard i 在积压 ≥ i×256KB 时参与分发);
/// 实际批量随积压自适应放大 (64KB ~ 1MiB), 推送上限 64MB/s/分片。
///
/// 取消方式: 前端对每个分片调用 unsubscribe_rawdata(channel_id)
#[tauri::command]
pub async fn subscribe_rawdata(
    state: State<'_, AppState>,
    on_event: Channel<RawDataBatch>,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_bytes: Option<usize>,
) -> Result<String> {
    let interval = Duration::from_millis(interval_ms.unwrap_or(16));
    let max_n = max_bytes.unwrap_or(65536);
    let channel_id = on_event.id();
    let collector = state.raw_data_collector.clone();

    if group_id.is_none() {
        let c = state.raw_data_collector.lock();
        log::info!(
            "subscribe_rawdata: 新订阅组, 起始积压={}B ({} chunks, base_index={})",
            c.stored_bytes(),
            c.chunk_count(),
            c.base_index()
        );
    }

    let (source, seq, shard_idx, group_key) = crate::pipeline::stream::join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || crate::pipeline::stream::RawDataSource::new(collector),
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.raw_data_tasks.lock().insert(channel_id, cancel_tx);

    let groups = state.stream_groups.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        crate::pipeline::stream::sharded_stream_loop(
            format!("原始数据分片{}", shard_idx),
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

/// 取消订阅原始数据
#[tauri::command]
pub async fn unsubscribe_rawdata(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    if let Some(tx) = state.raw_data_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

/// 订阅指定 FrameDecoder 节点的原始数据 — 统一分片流
///
/// 与 subscribe_rawdata (全局原始字节流) 不同, 本命令只推送指定 FrameDecoder 节点
/// 在 feed_frame_decoders_cached 中每帧消费的原始字节 (frame.raw_bytes), 供前端 RawData
/// 以独立通道 (旁路) 展示每个节点的原始帧内容, 不影响全局 f32 图快照。
///
/// node_id: FrameDecoder widget id; 若节点不存在则返回空字符串 (no-op, 不分片)
/// interval_ms: 推送间隔 (毫秒), 默认 16ms
/// max_bytes: 单次推送最小批量, 默认 65536 (随积压自适应放大)
///
/// 取消方式: 前端对每个分片调用 unsubscribe_rawdata_node(channel_id)
#[tauri::command]
pub async fn subscribe_rawdata_node(
    state: State<'_, AppState>,
    node_id: String,
    on_event: Channel<RawDataBatch>,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_bytes: Option<usize>,
) -> Result<String> {
    // 节点不存在 → no-op, 返回空组 id (前端不再加分片)
    let collector = match state.decoder_raw_collectors.lock().get(&node_id) {
        Some(c) => c.clone(),
        None => return Ok(String::new()),
    };
    let interval = Duration::from_millis(interval_ms.unwrap_or(16));
    let max_n = max_bytes.unwrap_or(65536);
    let channel_id = on_event.id();

    let (source, seq, shard_idx, group_key) = crate::pipeline::stream::join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || crate::pipeline::stream::RawDataSource::new(collector),
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.raw_data_node_tasks.lock().insert(channel_id, cancel_tx);

    let groups = state.stream_groups.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        crate::pipeline::stream::sharded_stream_loop(
            format!("节点原始数据分片{}", shard_idx),
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

/// 取消订阅指定 FrameDecoder 节点的原始数据
#[tauri::command]
pub async fn unsubscribe_rawdata_node(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    if let Some(tx) = state.raw_data_node_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

/// 解析方向过滤字符串
fn parse_direction_filter(s: &str) -> DirectionFilter {
    match s.to_lowercase().as_str() {
        "rx" => DirectionFilter::Rx,
        "tx" => DirectionFilter::Tx,
        _ => DirectionFilter::All,
    }
}

/// 订阅带方向与搜索过滤的原始数据 — 统一分片流
///
/// 与 subscribe_rawdata 的区别: 后端只推送方向匹配且包含搜索模式的 chunk,
/// 前端无需再遍历过滤, 适合 20MB/s 以上高码率场景。
///
/// direction: "all" | "rx" | "tx"
/// search: 搜索字符串; 空串表示不过滤; 纯 hex 字符按 hex 解析, 其他按 ascii 解析
#[tauri::command]
pub async fn subscribe_rawdata_filtered(
    state: State<'_, AppState>,
    on_event: Channel<RawDataBatch>,
    direction: String,
    search: String,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_bytes: Option<usize>,
) -> Result<String> {
    let interval = Duration::from_millis(interval_ms.unwrap_or(16));
    let max_n = max_bytes.unwrap_or(65536);
    let channel_id = on_event.id();
    let collector = state.raw_data_collector.clone();
    let direction = parse_direction_filter(&direction);
    let search = if search.trim().is_empty() { None } else { Some(search) };

    if group_id.is_none() {
        let c = state.raw_data_collector.lock();
        log::info!(
            "subscribe_rawdata_filtered: direction={:?}, search={:?}, 起始积压={}B ({} chunks)",
            direction,
            search,
            c.stored_bytes(),
            c.chunk_count()
        );
    }

    let (source, seq, shard_idx, group_key) = crate::pipeline::stream::join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || crate::pipeline::filtered_sources::FilteredRawDataSource::new(collector, direction, search.as_deref()),
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.raw_data_tasks.lock().insert(channel_id, cancel_tx);

    let groups = state.stream_groups.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        crate::pipeline::stream::sharded_stream_loop(
            format!("过滤原始数据分片{}", shard_idx),
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

/// 订阅带方向与搜索过滤的 FrameDecoder 节点原始数据 — 统一分片流
#[tauri::command]
pub async fn subscribe_rawdata_node_filtered(
    state: State<'_, AppState>,
    node_id: String,
    on_event: Channel<RawDataBatch>,
    direction: String,
    search: String,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_bytes: Option<usize>,
) -> Result<String> {
    let collector = match state.decoder_raw_collectors.lock().get(&node_id) {
        Some(c) => c.clone(),
        None => return Ok(String::new()),
    };
    let interval = Duration::from_millis(interval_ms.unwrap_or(16));
    let max_n = max_bytes.unwrap_or(65536);
    let channel_id = on_event.id();
    let direction = parse_direction_filter(&direction);
    let search = if search.trim().is_empty() { None } else { Some(search) };

    let (source, seq, shard_idx, group_key) = crate::pipeline::stream::join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || crate::pipeline::filtered_sources::FilteredRawDataSource::new(collector, direction, search.as_deref()),
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.raw_data_node_tasks.lock().insert(channel_id, cancel_tx);

    let groups = state.stream_groups.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        crate::pipeline::stream::sharded_stream_loop(
            format!("过滤节点原始数据分片{}", shard_idx),
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

/// 清空原始数据收集器 (全局 + 各 FrameDecoder 节点旁路收集器)
#[tauri::command]
pub async fn clear_raw_data_collector(state: State<'_, AppState>) -> Result<()> {
    state.raw_data_collector.lock().clear();
    for collector in state.decoder_raw_collectors.lock().values() {
        collector.lock().clear();
    }
    Ok(())
}
