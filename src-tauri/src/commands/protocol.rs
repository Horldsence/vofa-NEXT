use crate::state::AppState;
use tauri::{AppHandle, State};
use vofa_next_core::{ConnectionState, Error, ProtocolConfig, Result, TransportConfig};

/// 设置指定 Protocol 节点的协议配置 (运行时覆盖, 重建解析引擎)
///
/// 注意: 图是协议配置的权威来源 — 下次 update_tab_graph 时若图中该节点
/// 配置与本值不一致, 引擎会按图配置再次重建 (见 DataPlaneState::sync_protocol_states)。
///
/// 如果当前有 TestData 连接, 自动断开 (全部 TestData 节点)。
/// TestData 生成器只在 `open()` 时接收协议参数, 中连换协议会导致
/// 生成格式与解析引擎不匹配, 因此强制断连让用户 reconnect。
#[tauri::command]
pub async fn set_protocol(
    app: AppHandle,
    state: State<'_, AppState>,
    node_id: String,
    config: ProtocolConfig,
) -> Result<()> {
    // 协议变化 → TestData 生成格式失效: 断开所有打开的 TestData 传输
    {
        let mut manager = state.transport.lock().await;
        let test_nodes: Vec<String> = manager
            .list_open()
            .into_iter()
            .filter(|id| matches!(manager.config(id), Some(TransportConfig::TestData(_))))
            .collect();
        if !test_nodes.is_empty() {
            for id in &test_nodes {
                state.data_plane.detach(id);
                manager.close(id);
                crate::events::emit_transport_state(&app, id, ConnectionState::Disconnected);
            }
            log::info!("协议切换: 自动断开 TestData 连接 ({} 个), 请重新连接", test_nodes.len());
        }
    }

    let st = state
        .data_plane
        .protocol_states
        .lock()
        .get(&node_id)
        .cloned()
        .ok_or_else(|| Error::Config(format!("协议节点不存在: {}", node_id)))?;
    {
        let mut s = st.lock();
        s.engine = std::sync::Arc::new(parking_lot::Mutex::new(
            vofa_next_protocol::create_engine(&config),
        ));
        s.config = config;
        s.parallel_supported = None;
        s.in_parallel = false;
        s.detection_notified = false;
        s.parallel = std::sync::Arc::new(tokio::sync::Mutex::new(
            crate::pipeline::feed_parallel::ParallelFeeder::new(),
        ));
    }
    Ok(())
}

/// 获取指定 Protocol 节点的当前协议配置
#[tauri::command]
pub async fn get_protocol(state: State<'_, AppState>, node_id: String) -> Result<ProtocolConfig> {
    let st = state
        .data_plane
        .protocol_states
        .lock()
        .get(&node_id)
        .cloned()
        .ok_or_else(|| Error::Config(format!("协议节点不存在: {}", node_id)))?;
    let config = st.lock().config.clone();
    Ok(config)
}

/// 获取自动检测到的通道数 (仅在自动模式下返回 Some, 手动模式返回 None)
#[tauri::command]
pub async fn get_detected_channels(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<Option<usize>> {
    let st = state
        .data_plane
        .protocol_states
        .lock()
        .get(&node_id)
        .cloned()
        .ok_or_else(|| Error::Config(format!("协议节点不存在: {}", node_id)))?;
    let engine = st.lock().engine.clone();
    let detected = engine.lock().detected_channels();
    Ok(detected)
}
