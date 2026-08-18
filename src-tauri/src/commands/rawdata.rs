use crate::state::AppState;
use std::time::Duration;
use tauri::{ipc::Channel, State};
use vofa_next_buffer::{DirectionFilter, RawDataBatch};
use vofa_next_core::Result;

// ============ 原始数据命令 ============

/// 解析方向过滤字符串
fn parse_direction_filter(s: &str) -> DirectionFilter {
    match s.to_lowercase().as_str() {
        "rx" => DirectionFilter::Rx,
        "tx" => DirectionFilter::Tx,
        _ => DirectionFilter::All,
    }
}

/// 订阅原始数据 — 统一分片流 (增量 drain + 自动并发分片)
///
/// - 首次调用不传 `group_id`: 创建订阅组 (本通道为 shard 0), 返回组 id
/// - 后续调用传入 `group_id`: 作为新 shard 加入 (最多 MAX_STREAM_SHARDS 个)
///
/// source: Transport 节点 id (每源一个 RawDataCollector, rx/tx 都进该实例)
/// 分片按积压自动激活/休眠 (shard i 在积压 ≥ i×256KB 时参与分发);
/// 实际批量随积压自适应放大 (64KB ~ 1MiB), 推送上限 64MB/s/分片。
///
/// 取消方式: 前端对每个分片调用 unsubscribe_rawdata(channel_id)
#[tauri::command]
pub async fn subscribe_rawdata(
    state: State<'_, AppState>,
    source: String,
    on_event: Channel<RawDataBatch>,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_bytes: Option<usize>,
) -> Result<String> {
    let interval = Duration::from_millis(interval_ms.unwrap_or(16));
    let max_n = max_bytes.unwrap_or(65536);
    let channel_id = on_event.id();
    let collector = state.data_plane.raw_collector_for(&source);

    if group_id.is_none() {
        let c = collector.lock();
        log::info!(
            "subscribe_rawdata({}): 新订阅组, 起始积压={}B ({} chunks, base_index={})",
            source,
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
    state
        .raw_data_node_tasks
        .lock()
        .insert(channel_id, cancel_tx);

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

/// 订阅带方向与搜索过滤的原始数据 — 统一分片流
///
/// 与 subscribe_rawdata 的区别: 后端只推送方向匹配且包含搜索模式的 chunk,
/// 前端无需再遍历过滤, 适合 20MB/s 以上高码率场景。
///
/// source: Transport 节点 id
/// direction: "all" | "rx" | "tx"
/// search: 搜索字符串; 空串表示不过滤; 纯 hex 字符按 hex 解析, 其他按 ascii 解析
#[tauri::command]
pub async fn subscribe_rawdata_filtered(
    state: State<'_, AppState>,
    source: String,
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
    let collector = state.data_plane.raw_collector_for(&source);
    let direction = parse_direction_filter(&direction);
    let search = if search.trim().is_empty() {
        None
    } else {
        Some(search)
    };

    if group_id.is_none() {
        let c = collector.lock();
        log::info!(
            "subscribe_rawdata_filtered({}): direction={:?}, search={:?}, 起始积压={}B ({} chunks)",
            source,
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
        || {
            crate::pipeline::filtered_sources::FilteredRawDataSource::new(
                collector,
                direction,
                search.as_deref(),
            )
        },
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
    let search = if search.trim().is_empty() {
        None
    } else {
        Some(search)
    };

    let (source, seq, shard_idx, group_key) = crate::pipeline::stream::join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || {
            crate::pipeline::filtered_sources::FilteredRawDataSource::new(
                collector,
                direction,
                search.as_deref(),
            )
        },
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state
        .raw_data_node_tasks
        .lock()
        .insert(channel_id, cancel_tx);

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
