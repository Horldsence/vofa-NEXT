use app_state::AppState;
use can_types::{CanFrame, CanFrameBatch, CanFrameFilter, CandleDeviceInfo};
use error::ConfigError;
use pipeline_dispatcher::filtered_sources::FilteredCanStreamSource;
use pipeline_stream::{join_or_create_group, leave_group, sharded_stream_loop, CanStreamSource};
use std::time::Duration;
use tauri::{ipc::Channel, State};
use vofa_core::Result;

// ============ CAN 帧相关 ============

/// 发送 CAN 帧
///
/// 通过指定 Protocol 节点引擎的 encode_can 编码为字节, 再经传输注册表发送。
/// 若该协议不是 CAN 协议 (encode_can 返回空), 直接返回 Ok。
///
/// - `node_id`: 目标 Transport 节点 id
/// - `protocol_node`: 编码用 Protocol 节点 id; None 时沿全局 BytePlan
///   查找该 Transport 下游的第一个 Protocol 节点
#[tauri::command]
pub async fn send_can_frame(
    state: State<'_, AppState>,
    node_id: String,
    protocol_node: Option<String>,
    frame: CanFrame,
) -> Result<()> {
    let plane = state.data_plane.clone();
    let proto_id = match protocol_node {
        Some(p) => Some(p),
        None => {
            // 沿字节平面找该 transport 下游第一个 Protocol 节点
            let routes = plane.byte_plan.lock().routes_for(&node_id).to_vec();
            let nodes = plane.global_nodes.lock();
            routes.iter().find_map(|r| {
                matches!(
                    nodes.get(&r.target).map(|n| &n.kind),
                    Some(node_kind::NodeKind::Protocol { .. })
                )
                .then(|| r.target.clone())
            })
        }
    };
    let Some(proto_id) = proto_id else {
        return Ok(()); // 无可用协议节点, 忽略
    };
    let data = {
        let st = plane
            .protocol_states
            .lock()
            .get(&proto_id)
            .cloned()
            .ok_or_else(|| {
                vofa_core::Error::Config(ConfigError::ProtocolNodeNotFound {
                    node_id: proto_id.clone(),
                })
            })?;
        let engine = st.lock().engine.clone();
        let bytes = engine.lock().encode_can(&frame);
        bytes
    };
    if data.is_empty() {
        return Ok(()); // 非 CAN 协议, 忽略
    }
    state.transport.lock().await.send(&node_id, &data)
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

    let (source, seq, shard_idx, group_key) = join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || CanStreamSource::new(buffer, max_n),
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.can_tasks.lock().insert(channel_id, cancel_tx);

    let groups = state.stream_groups.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        sharded_stream_loop(
            format!("CAN 帧分片{shard_idx}"),
            source,
            on_event,
            shard_idx,
            seq,
            interval,
            max_n,
            cancel_rx,
        )
        .await;
        leave_group(&groups, &exit_key);
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

/// 订阅带过滤条件的 CAN 帧推送 — 统一分片流
///
/// 与 subscribe_can_frames 的区别: 后端只推送匹配 filter 的帧,
/// 订阅游标从缓冲区最旧可读位置开始, 可先拉取全部历史匹配帧, 之后严格增量。
/// 取消方式相同: unsubscribe_can_frames(channel_id)
#[tauri::command]
pub async fn subscribe_can_frames_filtered(
    state: State<'_, AppState>,
    on_event: Channel<CanFrameBatch>,
    filter: CanFrameFilter,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_frames: Option<usize>,
) -> Result<String> {
    let interval = Duration::from_millis(interval_ms.unwrap_or(100));
    let max_n = max_frames.unwrap_or(500);
    let channel_id = on_event.id();
    let buffer = state.can_buffer.clone();

    let (source, seq, shard_idx, group_key) = join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || FilteredCanStreamSource::new(buffer, filter),
    )?;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.can_tasks.lock().insert(channel_id, cancel_tx);

    let groups = state.stream_groups.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        sharded_stream_loop(
            format!("过滤 CAN 帧分片{shard_idx}"),
            source,
            on_event,
            shard_idx,
            seq,
            interval,
            max_n,
            cancel_rx,
        )
        .await;
        leave_group(&groups, &exit_key);
    });
    Ok(group_key)
}

/// 同步查询: 获取最近 N 个 CAN 帧
///
/// 返回 `CanFrameBatch` (与订阅路径同构) — 前端 `getRecentCanFrames` 直接
/// 作为首屏快照灌入 buffer sink, 不需要再做结构转换。`seq: 0` 表示"非流式",
/// 下游收到后会立即消费, 后续若切换到订阅模式, 增量流从 seq=1 开始。
#[tauri::command]
pub async fn get_recent_can_frames(
    state: State<'_, AppState>,
    count: usize,
) -> Result<CanFrameBatch> {
    let frames = state.can_buffer.lock().get_recent(count);
    Ok(CanFrameBatch { seq: 0, frames })
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
    transport_can_bridge::candle::list_devices()
}
