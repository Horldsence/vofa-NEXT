use app_state::AppState;
use can_types::{CanFrame, CanFrameBatch, CandleDeviceInfo};
use error::ConfigError;
use tauri::State;
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
                    Some(kind::NodeKind::Protocol { .. })
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
    can_bridge::candle::list_devices()
}
