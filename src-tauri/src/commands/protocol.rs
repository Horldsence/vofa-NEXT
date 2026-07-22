use crate::state::AppState;
use tauri::{AppHandle, Emitter, State};
use vofa_next_core::{ConnectionState, ProtocolConfig, Result, TransportConfig};

/// 设置协议引擎
///
/// 如果当前是 TestData 连接且协议发生变化, 自动断开连接。
/// TestData 生成器只在 `open()` 时接收协议参数, 中连换协议会导致
/// 生成格式与解析引擎不匹配, 因此强制断连让用户 reconnect。
#[tauri::command]
pub async fn set_protocol(
    app: AppHandle,
    state: State<'_, AppState>,
    config: ProtocolConfig,
) -> Result<()> {
    // 如果当前是 TestData 连接, 自动断开以便 reconnect 时用新协议重建 test_data 生成器
    {
        let mut manager = state.transport.lock().await;
        let is_test_data = manager
            .config()
            .is_some_and(|c| matches!(c, TransportConfig::TestData(_)));
        let is_connected = manager.state() == ConnectionState::Connected;
        if is_test_data && is_connected {
            manager.close().await;
            let _ = app.emit("transport:state", ConnectionState::Disconnected);
            log::info!("协议切换: 自动断开 TestData 连接, 请重新连接");
        }
    }

    let engine = vofa_next_protocol::create_engine(&config);
    *state.protocol.lock() = engine;
    *state.protocol_config.lock() = config;
    Ok(())
}

/// 获取当前协议配置
#[tauri::command]
pub async fn get_protocol(state: State<'_, AppState>) -> Result<ProtocolConfig> {
    Ok(state.protocol_config.lock().clone())
}

/// 获取自动检测到的通道数 (仅在自动模式下返回 Some, 手动模式返回 None)
#[tauri::command]
pub async fn get_detected_channels(state: State<'_, AppState>) -> Result<Option<usize>> {
    Ok(state.protocol.lock().detected_channels())
}
