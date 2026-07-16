use crate::state::{data_loop, AppState};
use serial_core::{
    ConnectionState, PortInfo, ProtocolConfig, Result, TransportConfig, TransportStats,
    WidgetBinding,
};
use serial_transport::TransportManager;
use tauri::{AppHandle, Emitter, State};

/// 列出所有可用串口
#[tauri::command]
pub async fn list_ports() -> Result<Vec<PortInfo>> {
    TransportManager::list_ports()
}

/// 打开传输连接
#[tauri::command]
pub async fn open_transport(
    app: AppHandle,
    state: State<'_, AppState>,
    config: TransportConfig,
) -> Result<()> {
    let mut manager = state.transport.lock().await;
    manager.open(config).await?;

    let _ = app.emit("transport:state", ConnectionState::Connected);

    // 启动数据循环
    if let Some(rx) = manager.subscribe() {
        let protocol = state.protocol.clone();
        tokio::spawn(async move {
            data_loop(app, rx, protocol).await;
        });
    }

    Ok(())
}

/// 关闭传输连接
#[tauri::command]
pub async fn close_transport(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let mut manager = state.transport.lock().await;
    manager.close().await;
    let _ = app.emit("transport:state", ConnectionState::Disconnected);
    Ok(())
}

/// 发送原始字节
#[tauri::command]
pub async fn send_raw(state: State<'_, AppState>, data: Vec<u8>) -> Result<()> {
    let manager = state.transport.lock().await;
    manager.send(&data).await
}

/// 发送字符串
#[tauri::command]
pub async fn send_string(state: State<'_, AppState>, text: String) -> Result<()> {
    let manager = state.transport.lock().await;
    manager.send(text.as_bytes()).await
}

/// 发送控件值 (根据绑定模式自动编码)
#[tauri::command]
pub async fn send_widget_value(
    state: State<'_, AppState>,
    binding: WidgetBinding,
    value: f32,
) -> Result<()> {
    let data = match binding {
        WidgetBinding::None => return Ok(()),
        WidgetBinding::Auto { channel } => {
            state.protocol.lock().encode_channel(channel, value)
        }
        WidgetBinding::Manual { template } => {
            template.replace("{value}", &format!("{}", value)).into_bytes()
        }
    };

    // protocol lock 在此已释放
    let manager = state.transport.lock().await;
    manager.send(&data).await
}

/// 获取连接状态
#[tauri::command]
pub async fn get_connection_state(state: State<'_, AppState>) -> Result<ConnectionState> {
    let manager = state.transport.lock().await;
    Ok(manager.state())
}

/// 获取传输统计
#[tauri::command]
pub async fn get_stats(state: State<'_, AppState>) -> Result<TransportStats> {
    let manager = state.transport.lock().await;
    Ok(manager.stats())
}

/// 设置协议引擎
#[tauri::command]
pub async fn set_protocol(state: State<'_, AppState>, config: ProtocolConfig) -> Result<()> {
    let engine = serial_vofa::create_engine(&config);
    *state.protocol.lock() = engine;
    *state.protocol_config.lock() = config;
    Ok(())
}

/// 获取当前协议配置
#[tauri::command]
pub async fn get_protocol(state: State<'_, AppState>) -> Result<ProtocolConfig> {
    Ok(state.protocol_config.lock().clone())
}
