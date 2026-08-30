use app_state::AppState;
use buffer_raw::RawDataDirection;
use error::ConfigError;
use notify_events::emit_transport_state;
use notify_events::notify;
use schema_types::{ProtocolConfig, ProtocolSchema, TestDataLink};
use serde::Serialize;
use tauri::{AppHandle, State};
use transport_core::TransportManager;
use vofa_core::{
    ConnectionState, Error, PortInfo, Result, TransportConfig, TransportStats, WidgetBinding,
};

/// 列出所有可用串口
#[tauri::command]
pub async fn list_ports() -> Result<Vec<PortInfo>> {
    TransportManager::list_ports()
}

/// 打开传输连接 (node_id = 图中 Transport 节点 id)
///
/// `protocol` 与 `schema` 仅被 TestData 用作生成数据的线缆格式参考, 其他传输类型忽略。
/// schema = Custom 且带 encode 块时按 schema 编码, 否则走 legacy protocol 路径。
/// 成功后挂载数据平面读任务 (subscribe → 字节路由 → 数值平面)。
#[tauri::command]
pub async fn open_transport(
    app: AppHandle,
    state: State<'_, AppState>,
    node_id: String,
    config: TransportConfig,
    protocol: ProtocolConfig,
    schema: Option<ProtocolSchema>,
) -> Result<()> {
    let kind = notify::transport_kind_str(&config);
    {
        let mut manager = state.transport.lock().await;
        let link = TestDataLink { protocol, schema };
        if let Err(e) = manager.open(&node_id, config, link).await {
            log::error!("连接失败 ({node_id}): {e}");
            notify::error(&app, format!("连接失败: {e}"));
            return Err(e);
        }
    }

    emit_transport_state(&app, &node_id, ConnectionState::Connected);
    log::info!("连接已建立: {kind} ({node_id})");
    notify::connected(&app, kind);

    // 挂载数据平面读任务 (每 Transport 节点一个)
    state.data_plane.attach(app, &node_id).await;
    Ok(())
}

/// 关闭传输连接 (node_id = 图中 Transport 节点 id)
#[tauri::command]
pub async fn close_transport(
    app: AppHandle,
    state: State<'_, AppState>,
    node_id: String,
) -> Result<()> {
    state.data_plane.mark_source_disconnected(&node_id);
    state.data_plane.detach(&node_id);
    state.transport.lock().await.close(&node_id);
    emit_transport_state(&app, &node_id, ConnectionState::Disconnected);
    log::info!("连接已关闭: {node_id}");
    notify::disconnected(&app);
    Ok(())
}

/// 发送原始字节 (node_id = 目标 Transport 节点 id)
#[tauri::command]
pub async fn send_raw(state: State<'_, AppState>, node_id: String, data: Vec<u8>) -> Result<()> {
    state.transport.lock().await.send(&node_id, &data)?;
    // TX 方向字节进该源的 raw 收集器 (收集器在 attach 时已建; 未打开时上面的 send 已报错)
    state
        .data_plane
        .raw_collector_for(&node_id)
        .lock()
        .push_chunk(vofa_core::now_us(), RawDataDirection::Tx, &data);
    Ok(())
}

/// 发送字符串 (node_id = 目标 Transport 节点 id)
#[tauri::command]
pub async fn send_string(state: State<'_, AppState>, node_id: String, text: String) -> Result<()> {
    send_raw(state, node_id, text.into_bytes()).await
}

/// TextOut 手动发送 — 立即把 textout 节点当前图内文本发往其目标 Transport
///
/// 与自动 ticker 共用数据源 (`graph_string_outputs[node]["text"]`) 与换行后缀;
/// 典型场景: 前端 Send 按钮在值未变化时也强制发送一次。
#[tauri::command]
pub async fn send_text_out_now(state: State<'_, AppState>, node_id: String) -> Result<()> {
    // 找到该 TextOut 的编译规格 (target_transport + newline 后缀; 图重编译间隙容错跳过)
    let spec = {
        let graphs = state.data_plane.eval.graphs.lock();
        graphs
            .values()
            .find_map(|g| {
                g.compiled()
                    .textouts()
                    .iter()
                    .find(|s| &*s.node_id == node_id.as_str())
                    .cloned()
            })
            .ok_or_else(|| {
                Error::Config(ConfigError::ProtocolNodeNotFound {
                    node_id: node_id.clone(),
                })
            })?
    };
    let text = state
        .data_plane
        .eval
        .graph_string_outputs
        .lock()
        .get(&node_id)
        .and_then(|p| p.get("text"))
        .cloned()
        .unwrap_or_default();
    let mut payload = text;
    payload.push_str(spec.newline_suffix);
    let target = spec.target_transport.to_string();
    state
        .transport
        .lock()
        .await
        .send(&target, payload.as_bytes())?;
    Ok(())
}

/// 发送控件值 (根据绑定模式自动编码)
///
/// - `node_id`: 目标 Transport 节点 id
/// - `protocol_node`: Auto 编码所用的 Protocol 节点 id (Manual 模式可传 None)
#[tauri::command]
pub async fn send_widget_value(
    state: State<'_, AppState>,
    node_id: String,
    protocol_node: Option<String>,
    binding: WidgetBinding,
    value: f32,
) -> Result<()> {
    let data = match binding {
        WidgetBinding::None => return Ok(()),
        WidgetBinding::Auto { channel } => {
            let pn = protocol_node
                .ok_or_else(|| Error::Config(ConfigError::AutoBindingMissingProtocolNode))?;
            let st = state
                .data_plane
                .protocol_states
                .lock()
                .get(&pn)
                .cloned()
                .ok_or_else(|| {
                    Error::Config(ConfigError::ProtocolNodeNotFound {
                        node_id: pn.clone(),
                    })
                })?;
            let engine = st.lock().engine.clone();
            let bytes = engine.lock().encode_channel(channel, value);
            bytes
        }
        WidgetBinding::Manual { template } => template
            .replace("{value}", &format!("{value}"))
            .into_bytes(),
    };
    send_raw(state, node_id, data).await
}

/// 获取连接状态 (未知节点返回 Disconnected)
#[tauri::command]
pub async fn get_connection_state(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<ConnectionState> {
    let manager = state.transport.lock().await;
    Ok(manager
        .state(&node_id)
        .unwrap_or(ConnectionState::Disconnected))
}

/// 获取传输统计 (未知节点返回全零)
#[tauri::command]
pub async fn get_stats(state: State<'_, AppState>, node_id: String) -> Result<TransportStats> {
    let manager = state.transport.lock().await;
    Ok(manager.stats(&node_id).unwrap_or_default())
}

/// 启动测试数据生成 (node_id = TestData Transport 节点 id)
#[tauri::command]
pub async fn start_test_data(state: State<'_, AppState>, node_id: String) -> Result<()> {
    let manager = state.transport.lock().await;
    manager.set_test_data_running(&node_id, true);
    Ok(())
}

/// 停止测试数据生成
#[tauri::command]
pub async fn stop_test_data(state: State<'_, AppState>, node_id: String) -> Result<()> {
    let manager = state.transport.lock().await;
    manager.set_test_data_running(&node_id, false);
    Ok(())
}

/// 获取测试数据生成状态
#[tauri::command]
pub async fn get_test_data_state(state: State<'_, AppState>, node_id: String) -> Result<bool> {
    let manager = state.transport.lock().await;
    Ok(manager.is_test_data_running(&node_id))
}

/// 运行时热更新传输节点的链路配置 (图连接/协议配置变化后由前端推送)
///
/// 当前仅 TestData 生成器消费链路配置 (protocol + 可选 schema); 其他传输类型的
/// 字节收发与协议无关, 静默接受。节点未打开时返回 PortNotFound, 前端据此 toast 提示用户重连。
#[tauri::command]
pub async fn update_transport_protocol(
    state: State<'_, AppState>,
    node_id: String,
    protocol: ProtocolConfig,
    schema: Option<ProtocolSchema>,
    test_data_config: Option<vofa_core::TestDataConfig>,
) -> Result<()> {
    state.transport.lock().await.update_link(
        &node_id,
        TestDataLink { protocol, schema },
        test_data_config,
    )
}

/// 协议回环：发送字节并立即捕获协议引擎解析结果
///
/// 用于协议调试场景 — 将用户构造的字节发送到 transport (node_id),
/// 同时直接调用指定 Protocol 节点 (protocol_node) 的引擎解析,
/// 返回发送字节与解析结果对照。
///
/// 所有 Transport 发送的字节都会经 TransportHandle::send 统一回环到
/// 本节点接收广播, 读任务也会再次解析; 本命令返回的是**即时同步**
/// 解析结果, 不等读任务管道。
#[derive(Debug, Clone, Serialize)]
pub struct LoopbackResult {
    pub sent_hex: String,
    pub rx_bytes: Vec<u8>,
    pub frame_count: usize,
    pub channels: Vec<f32>,
    pub can_count: usize,
}

#[tauri::command]
pub async fn send_and_capture(
    state: State<'_, AppState>,
    node_id: String,
    protocol_node: String,
    data: Vec<u8>,
) -> Result<LoopbackResult> {
    // 1. 发送到 transport (TestData 模式下回环)
    //    回环与串口开关无关: 未连接时跳过发送, 仅做本地解析对照
    {
        let manager = state.transport.lock().await;
        if manager.state(&node_id) == Some(ConnectionState::Connected) {
            manager.send(&node_id, &data)?;
        }
    }

    // 2. 即时调用协议引擎解析 (同步, 不依赖读任务管道)
    let st = state
        .data_plane
        .protocol_states
        .lock()
        .get(&protocol_node)
        .cloned()
        .ok_or_else(|| {
            Error::Config(ConfigError::ProtocolNodeNotFound {
                node_id: protocol_node.clone(),
            })
        })?;
    let out = st.lock().engine.lock().feed(&data);
    let frames = out.frames;
    let can_count = out.can_frames.len();

    Ok(LoopbackResult {
        sent_hex: data
            .iter()
            .map(|b| format!("{b:02X}"))
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
