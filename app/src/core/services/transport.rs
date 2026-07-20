use crate::core::notify;
use crate::core::state::AppState;
use serde::Serialize;
use vofa_next_core::{
    ConnectionState, PortInfo, Result, TransportConfig, TransportStats, WidgetBinding,
};
use vofa_next_transport::TransportManager;

/// 列出所有可用串口
pub async fn list_ports() -> Result<Vec<PortInfo>> {
    TransportManager::list_ports()
}

/// 打开传输连接
pub async fn open_transport(state: &AppState, config: TransportConfig) -> Result<()> {
    let kind = notify::transport_kind_str(&config);
    // 读取当前协议配置 — TestData 需要按协议格式生成线缆字节
    let protocol = state.protocol_config.lock().clone();
    let mut manager = state.transport.lock().await;
    if let Err(e) = manager.open(config, protocol).await {
        tracing::error!("连接失败: {}", e);
        notify::error(format!("连接失败: {}", e));
        return Err(e);
    }

    *state.connection_state.lock() = ConnectionState::Connected;
    tracing::info!("连接已建立: {}", kind);
    notify::connected(kind);

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
        let stats = state.stats.clone();
        let connection_state = state.connection_state.clone();
        tokio::spawn(async move {
            crate::core::pipeline::data_loop(
                rx,
                protocol,
                buffer,
                eval_state,
                raw_data_collector,
                can_buffer,
                can_load_stats,
                logic_buffer,
                decoded_buffer,
                stats,
                connection_state,
            )
            .await;
        });
    }

    Ok(())
}

/// 关闭传输连接
pub async fn close_transport(state: &AppState) -> Result<()> {
    let mut manager = state.transport.lock().await;
    manager.close().await;
    *state.connection_state.lock() = ConnectionState::Disconnected;
    tracing::info!("连接已关闭");
    notify::disconnected();
    Ok(())
}

/// 发送原始字节
pub async fn send_raw(state: &AppState, data: Vec<u8>) -> Result<()> {
    let manager = state.transport.lock().await;
    manager.send(&data).await
}

/// 发送字符串
pub async fn send_string(state: &AppState, text: String) -> Result<()> {
    let manager = state.transport.lock().await;
    manager.send(text.as_bytes()).await
}

/// 发送控件值 (根据绑定模式自动编码)
pub async fn send_widget_value(state: &AppState, binding: WidgetBinding, value: f32) -> Result<()> {
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

/// 获取连接状态 (从共享状态读取, 与 UI 每帧轮询的值一致)
pub async fn get_connection_state(state: &AppState) -> Result<ConnectionState> {
    Ok(*state.connection_state.lock())
}

/// 获取传输统计 (从共享状态读取, data_loop 节流写入)
pub async fn get_stats(state: &AppState) -> Result<TransportStats> {
    Ok(state.stats.lock().clone())
}

/// 启动测试数据生成
pub async fn start_test_data(state: &AppState) -> Result<()> {
    let manager = state.transport.lock().await;
    manager.set_test_data_running(true);
    Ok(())
}

/// 停止测试数据生成
pub async fn stop_test_data(state: &AppState) -> Result<()> {
    let manager = state.transport.lock().await;
    manager.set_test_data_running(false);
    Ok(())
}

/// 获取测试数据生成状态
pub async fn get_test_data_state(state: &AppState) -> Result<bool> {
    let manager = state.transport.lock().await;
    Ok(manager.is_test_data_running())
}

/// 协议回环：发送字节并立即捕获协议引擎解析结果
///
/// 用于协议调试场景 — 将用户构造的字节发送到 transport,
/// 同时直接调用协议引擎解析, 返回发送字节与解析结果对照。
///
/// TestData 模式: 发送的字节通过 transport 回环, data_loop 也会再次解析;
/// 本函数返回的是**即时同步**解析结果, 不等 data_loop 管道。
#[derive(Debug, Clone, Serialize)]
pub struct LoopbackResult {
    pub sent_hex: String,
    pub rx_bytes: Vec<u8>,
    pub frame_count: usize,
    pub channels: Vec<f32>,
    pub can_count: usize,
}

pub async fn send_and_capture(state: &AppState, data: Vec<u8>) -> Result<LoopbackResult> {
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
        sent_hex: data
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" "),
        rx_bytes: data.clone(),
        frame_count: frames.len(),
        channels: frames
            .first()
            .map(|f| f.channels.clone())
            .unwrap_or_default(),
        can_count,
    })
}
