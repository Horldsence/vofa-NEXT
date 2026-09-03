use app_state::AppState;
use data_plane::feed_parallel::ParallelFeeder;
use error::ConfigError;
use logic_decoder::LogicDecoderEngine;
use notify_events::emit_transport_state;
use protocol_can_bridge::{CandleEngine as Candle, RawDataEngine as RawData, SlcanEngine as Slcan};
use protocol_engine::ProtocolEngine;
use protocol_float::{FireWaterEngine as FireWater, JustFloatEngine as JustFloat};
use schema_types::ProtocolConfig;
use tauri::{AppHandle, State};
use vofa_core::{ConnectionState, Error, Result, TransportConfig};

/// 根据配置创建协议引擎
pub fn create_engine(config: &ProtocolConfig) -> Box<dyn ProtocolEngine> {
    match config {
        ProtocolConfig::JustFloat { channels } => Box::new(JustFloat::new(*channels)),
        ProtocolConfig::FireWater { channels } => Box::new(FireWater::new(*channels)),
        ProtocolConfig::RawData => Box::new(RawData::new()),
        ProtocolConfig::Slcan => Box::new(Slcan::new()),
        ProtocolConfig::CandleLight => Box::new(Candle::new()),
        ProtocolConfig::LogicDecode { decoder } => {
            Box::new(LogicDecoderEngine::new(decoder.clone()))
        }
        ProtocolConfig::Diagnostic { .. } => Box::new(RawData::new()),
    }
}

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
                emit_transport_state(&app, id, ConnectionState::Disconnected);
            }
            log::info!(
                "协议切换: 自动断开 TestData 连接 ({} 个), 请重新连接",
                test_nodes.len()
            );
        }
    }

    let st = state
        .data_plane
        .protocol_states
        .lock()
        .get(&node_id)
        .cloned()
        .ok_or_else(|| {
            Error::Config(ConfigError::ProtocolNodeNotFound {
                node_id: node_id.clone(),
            })
        })?;
    {
        let mut s = st.lock();
        s.engine = std::sync::Arc::new(parking_lot::Mutex::new(create_engine(&config)));
        s.config = config;
        s.parallel_supported = None;
        s.in_parallel = false;
        s.detection_notified = false;
        s.last_detected_pushed = None;
        s.parallel = std::sync::Arc::new(tokio::sync::Mutex::new(ParallelFeeder::new()));
    }
    // 手动通道数: 直接在后端对齐该源 buffer 通道数
    // (自动模式无需处理: 检测推送记录已重置, 重新检测到值后按变化推送时对齐)
    let manual_channels = st.lock().config.manual_channels();
    if let Some(n) = manual_channels {
        state.data_plane.buffer_for(&node_id).lock().set_channels(n);
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
        .ok_or_else(|| {
            Error::Config(ConfigError::ProtocolNodeNotFound {
                node_id: node_id.clone(),
            })
        })?;
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
        .ok_or_else(|| {
            Error::Config(ConfigError::ProtocolNodeNotFound {
                node_id: node_id.clone(),
            })
        })?;
    let engine = st.lock().engine.clone();
    let detected = engine.lock().detected_channels();
    Ok(detected)
}
