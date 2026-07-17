use crate::notify;
use crate::state::{data_loop, AppState};
use std::time::Duration;
use tauri::{ipc::Channel, AppHandle, Emitter, State};
use vofa_next_buffer::{graph::Edge, WaveformWindow};
use vofa_next_core::{
    ConnectionState, PortInfo, ProtocolConfig, Result, TransportConfig, TransportStats,
    WidgetBinding,
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
    let mut manager = state.transport.lock().await;
    if let Err(e) = manager.open(config).await {
        log::error!("连接失败: {}", e);
        notify::error(&app, format!("连接失败: {}", e));
        return Err(e);
    }

    let _ = app.emit("transport:state", ConnectionState::Connected);
    log::info!("连接已建立: {}", kind);
    notify::connected(&app, kind);

    // 启动数据循环
    if let Some(rx) = manager.subscribe() {
        let protocol = state.protocol.clone();
        let buffer = state.buffer.clone();
        tokio::spawn(async move {
            data_loop(app, rx, protocol, buffer).await;
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

/// 订阅波形数据 — 通过 Tauri Channel 推送窗口数据
///
/// interval_ms: 推送间隔 (毫秒), 默认 33ms (~30 FPS)
/// max_points: 单次推送的最大点数, 默认 1000
#[tauri::command]
pub async fn subscribe_waveform(
    state: State<'_, AppState>,
    on_event: Channel<WaveformWindow>,
    interval_ms: Option<u64>,
    max_points: Option<usize>,
) -> Result<()> {
    let buffer = state.buffer.clone();
    let interval = Duration::from_millis(interval_ms.unwrap_or(33));
    let max_pts = max_points.unwrap_or(1000);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        log::info!("波形订阅已启动, 间隔={}ms", interval.as_millis());
        loop {
            ticker.tick().await;
            let window = {
                let buf = buffer.lock();
                let pts = buf.point_count().min(max_pts);
                buf.get_recent(pts)
            };
            // Channel 已关闭则退出
            if on_event.send(window).is_err() {
                log::info!("波形订阅通道已关闭");
                break;
            }
        }
    });

    Ok(())
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

/// 更新节点图连接 (整体替换)
///
/// 若检测到循环, 返回错误
#[tauri::command]
pub async fn update_node_graph(state: State<'_, AppState>, edges: Vec<Edge>) -> Result<()> {
    let mut graph = state.graph.lock();
    // 先检查循环
    let mut temp = vofa_next_buffer::NodeGraph::new();
    temp.update_edges(edges);
    if temp.has_cycle() {
        return Err(vofa_next_core::Error::Config("节点图存在循环连接".into()));
    }
    *graph = temp;
    Ok(())
}

/// 获取当前节点图所有边
#[tauri::command]
pub async fn get_node_edges(state: State<'_, AppState>) -> Result<Vec<Edge>> {
    Ok(state.graph.lock().edges().to_vec())
}
