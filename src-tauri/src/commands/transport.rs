use crate::notify;
use crate::state::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use vofa_next_core::{
    ConnectionState, PortInfo, Result, TransportConfig, TransportStats, WidgetBinding,
};
use vofa_next_transport::TransportManager;

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
    let kind = notify::transport_kind_str(&config);
    // 读取当前协议配置 — TestData 需要按协议格式生成线缆字节
    let protocol = state.protocol_config.lock().clone();
    let mut manager = state.transport.lock().await;
    if let Err(e) = manager.open(config, protocol).await {
        log::error!("连接失败: {}", e);
        notify::error(&app, format!("连接失败: {}", e));
        return Err(e);
    }

    let _ = app.emit("transport:state", ConnectionState::Connected);
    log::info!("连接已建立: {}", kind);
    notify::connected(&app, kind);

    // 启动数据循环 (传入图评估所需的 Arc 字段)
    if let Some(rx) = manager.subscribe() {
        let protocol = state.protocol.clone();
        let buffer = state.buffer.clone();
        let eval_state = state.eval_state();
        let raw_data_collector = state.raw_data_collector.clone();
        let can_buffer = state.can_buffer.clone();
        let can_load_stats = state.can_load_stats.clone();
        let logic_buffer = state.logic_buffer.clone();
        let decoded_buffer = state.decoded_buffer.clone();
        tokio::spawn(async move {
            crate::state::data_loop(
                app,
                rx,
                protocol,
                buffer,
                eval_state,
                raw_data_collector,
                can_buffer,
                can_load_stats,
                logic_buffer,
                decoded_buffer,
            )
            .await;
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
    log::info!("连接已关闭");
    notify::disconnected(&app);
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
        WidgetBinding::Auto { channel } => state.protocol.lock().encode_channel(channel, value),
        WidgetBinding::Manual { template } => template
            .replace("{value}", &format!("{}", value))
            .into_bytes(),
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

/// 启动测试数据生成
#[tauri::command]
pub async fn start_test_data(state: State<'_, AppState>) -> Result<()> {
    let manager = state.transport.lock().await;
    manager.set_test_data_running(true);
    Ok(())
}

/// 停止测试数据生成
#[tauri::command]
pub async fn stop_test_data(state: State<'_, AppState>) -> Result<()> {
    let manager = state.transport.lock().await;
    manager.set_test_data_running(false);
    Ok(())
}

/// 获取测试数据生成状态
#[tauri::command]
pub async fn get_test_data_state(state: State<'_, AppState>) -> Result<bool> {
    let manager = state.transport.lock().await;
    Ok(manager.is_test_data_running())
}

/// 协议回环：发送字节并立即捕获协议引擎解析结果
///
/// 用于协议调试场景 — 将用户构造的字节发送到 transport,
/// 同时直接调用协议引擎解析, 返回发送字节与解析结果对照。
///
/// TestData 模式: 发送的字节通过 transport 回环, data_loop 也会再次解析;
/// 本命令返回的是**即时同步**解析结果, 不等 data_loop 管道。
#[derive(Debug, Clone, Serialize)]
pub struct LoopbackResult {
    pub sent_hex: String,
    pub rx_bytes: Vec<u8>,
    pub frame_count: usize,
    pub channels: Vec<f32>,
    pub can_count: usize,
}

#[tauri::command]
#[allow(dead_code)]
pub async fn send_and_capture(
    state: State<'_, AppState>,
    data: Vec<u8>,
) -> Result<LoopbackResult> {
    // 1. 发送到 transport (TestData 模式下回环)
    {
        let manager = state.transport.lock().await;
        manager.send(&data).await?;
    }

    // 2. 即时调用协议引擎解析 (同步, 不依赖 data_loop 管道)
    let mut proto = state.protocol.lock();
    let frames = proto.feed(&data);
    let can_count = proto.feed_can(&data).len();

    Ok(LoopbackResult {
        sent_hex: data.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" "),
        rx_bytes: data.clone(),
        frame_count: frames.len(),
        channels: frames.first().map(|f| f.channels.clone()).unwrap_or_default(),
        can_count,
    })
}
