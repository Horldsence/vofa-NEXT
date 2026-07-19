use crate::state::AppState;
use tauri::State;
use vofa_next_core::{ProtocolConfig, Result};

/// 设置协议引擎
#[tauri::command]
pub async fn set_protocol(state: State<'_, AppState>, config: ProtocolConfig) -> Result<()> {
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
