use crate::notify;
use crate::state::{data_loop, AppState, CustomInputBatch, GraphOutputSnapshot, SpectrumBatch};
use std::time::Duration;
use tauri::{ipc::Channel, AppHandle, Emitter, Manager, State};
use vofa_next_buffer::{graph::Edge, RawDataBatch, WaveformWindow};
use vofa_next_core::{
    CanFrame, CanFrameBatch, CanLoadSnapshot, CandleDeviceInfo,
    ConnectionState, DecodedEventBatch, LogicSampleBatch, PortInfo, ProtocolConfig, Result,
    TransportConfig, TransportStats, WidgetBinding,
};
use vofa_next_nodes::NodeDef;
use vofa_next_protocol::InputFormat;
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
            data_loop(app, rx, protocol, buffer, eval_state, raw_data_collector, can_buffer, can_load_stats, logic_buffer, decoded_buffer).await;
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
///
/// 取消方式: 前端调用 unsubscribe_waveform(channel_id) 触发 oneshot 取消信号,
/// task 在 select! 中收到信号后优雅退出, 避免向已关闭的 channel send 产生警告。
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
    let channel_id = on_event.id();

    // 创建取消信号 channel, 存入全局 state 供 unsubscribe_waveform 使用
    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.waveform_tasks.lock().insert(channel_id, cancel_tx);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        log::info!("波形订阅已启动, channel_id={}, 间隔={}ms", channel_id, interval.as_millis());
        loop {
            tokio::select! {
                // 收到取消信号 → 优雅退出
                _ = &mut cancel_rx => {
                    log::info!("波形订阅被主动取消, channel_id={}", channel_id);
                    break;
                }
                _ = ticker.tick() => {
                    let window = {
                        let buf = buffer.lock();
                        let pts = buf.point_count().min(max_pts);
                        buf.get_recent(pts)
                    };
                    // Channel 已关闭则退出
                    if on_event.send(window).is_err() {
                        log::info!("波形订阅通道已关闭, channel_id={}", channel_id);
                        break;
                    }
                }
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

/// 设置波形缓冲区最大点数
#[tauri::command]
pub async fn set_waveform_buffer_capacity(
    state: State<'_, AppState>,
    max_points: usize,
) -> Result<()> {
    state.buffer.lock().set_max_points(max_points);
    Ok(())
}

/// 设置原始数据收集器容量 (字节)
#[tauri::command]
pub async fn set_rawdata_buffer_capacity(
    state: State<'_, AppState>,
    capacity: usize,
) -> Result<()> {
    state.raw_data_collector.lock().set_capacity(capacity);
    Ok(())
}

/// 设置 CAN 帧缓冲区最大帧数
#[tauri::command]
pub async fn set_can_buffer_capacity(
    state: State<'_, AppState>,
    capacity: usize,
) -> Result<()> {
    state.can_buffer.lock().set_max_size(capacity);
    Ok(())
}

/// 设置逻辑采样缓冲区最大采样数
#[tauri::command]
pub async fn set_logic_buffer_capacity(
    state: State<'_, AppState>,
    capacity: usize,
) -> Result<()> {
    state.logic_buffer.lock().set_max_size(capacity);
    Ok(())
}

// ============ 节点图 (后端化重构) ============

/// 更新指定 tab 的节点图 (整体替换 nodes + edges)
///
/// 编译失败 (循环等) 返回错误, 旧图保留
#[tauri::command]
pub async fn update_tab_graph(
    state: State<'_, AppState>,
    tab_id: String,
    nodes: Vec<NodeDef>,
    edges: Vec<Edge>,
) -> Result<()> {
    let compiled = vofa_next_nodes::CompiledGraph::compile(tab_id.clone(), nodes, edges)
        .map_err(|e| vofa_next_core::Error::Config(format!("{}", e)))?;
    let mut graphs = state.graphs.lock();
    graphs.insert(tab_id, compiled);
    Ok(())
}

/// 移除指定 tab 的节点图 (tab 删除时调用)
#[tauri::command]
pub async fn remove_tab_graph(state: State<'_, AppState>, tab_id: String) -> Result<()> {
    state.graphs.lock().remove(&tab_id);
    Ok(())
}

/// 设置输入控件当前值 (Knob/Slider/Button/Radio/Checkbox 拖动时调用)
///
/// 该值会在下一帧 evaluate 时作为 Input 节点的输出
#[tauri::command]
pub async fn set_input_value(
    state: State<'_, AppState>,
    widget_id: String,
    value: f32,
) -> Result<()> {
    state.input_values.lock().insert(widget_id, value);
    Ok(())
}

/// 提交 Custom widget 的输出 (前端 iframe 调用 ctx.send 后回传)
///
/// 后端在下一帧 evaluate 时使用这些值作为 Custom 节点的输出
#[tauri::command]
pub async fn submit_custom_output(
    state: State<'_, AppState>,
    widget_id: String,
    outputs: std::collections::HashMap<String, f32>,
) -> Result<()> {
    state.custom_outputs.lock().insert(widget_id, outputs);
    Ok(())
}

/// 订阅图输出快照 — 60 FPS 推送 HashMap<widgetId, HashMap<portId, value>>
///
/// 前端通过单一订阅获取所有节点的实时输出值
#[tauri::command]
pub async fn subscribe_graph_outputs(
    state: State<'_, AppState>,
    on_event: Channel<GraphOutputSnapshot>,
) -> Result<()> {
    state.output_subscribers.lock().push(on_event);
    Ok(())
}

/// 订阅 Custom widget 输入批次 — 30 FPS 推送
///
/// 前端收到后转发到对应 iframe
#[tauri::command]
pub async fn subscribe_custom_inputs(
    state: State<'_, AppState>,
    on_event: Channel<CustomInputBatch>,
) -> Result<()> {
    state.custom_input_subscribers.lock().push(on_event);
    Ok(())
}

/// 订阅频谱分析结果 — 30 FPS 推送 SpectrumBatch
///
/// 前端 SpectrumChart 通过此订阅获取所有 SpectrumSink 节点的最新 FFT 结果。
/// batch.spectra: HashMap<sinkWidgetId, SpectrumResult>
/// 即使某 sink 的窗口未填满 (尚未产生新结果), 也会推送 snapshot 中的上一帧值,
/// 保证新订阅者立即收到数据, 图表连续不闪烁。
#[tauri::command]
pub async fn subscribe_spectrum(
    state: State<'_, AppState>,
    on_event: Channel<SpectrumBatch>,
) -> Result<()> {
    state.spectrum_subscribers.lock().push(on_event);
    Ok(())
}

/// 取消订阅图输出 — 从订阅者列表中移除指定 channel
///
/// 前端在取消订阅时应先调用此命令移除后端引用, 再注销 JS 端回调,
/// 避免后端向已关闭的 channel 发送数据时产生 "Couldn't find callback id" 警告。
#[tauri::command]
pub async fn unsubscribe_graph_outputs(
    state: State<'_, AppState>,
    channel_id: u32,
) -> Result<()> {
    let mut subs = state.output_subscribers.lock();
    subs.retain(|ch| ch.id() != channel_id);
    Ok(())
}

/// 取消订阅 Custom 输入 — 从订阅者列表中移除指定 channel
#[tauri::command]
pub async fn unsubscribe_custom_inputs(
    state: State<'_, AppState>,
    channel_id: u32,
) -> Result<()> {
    let mut subs = state.custom_input_subscribers.lock();
    subs.retain(|ch| ch.id() != channel_id);
    Ok(())
}

/// 取消订阅频谱 — 从订阅者列表中移除指定 channel
#[tauri::command]
pub async fn unsubscribe_spectrum(
    state: State<'_, AppState>,
    channel_id: u32,
) -> Result<()> {
    let mut subs = state.spectrum_subscribers.lock();
    subs.retain(|ch| ch.id() != channel_id);
    Ok(())
}

/// 取消订阅波形 — 通过 channel_id 触发 oneshot 取消信号, 让 task 优雅退出
#[tauri::command]
pub async fn unsubscribe_waveform(
    state: State<'_, AppState>,
    channel_id: u32,
) -> Result<()> {
    if let Some(tx) = state.waveform_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

/// 订阅原始数据 — 通过 Channel 周期性推送 RawDataBatch
///
/// interval_ms: 推送间隔 (毫秒), 默认 16ms (~60 FPS)
/// max_bytes: 单次推送的最大字节数, 默认 65536
#[tauri::command]
pub async fn subscribe_rawdata(
    state: State<'_, AppState>,
    on_event: Channel<RawDataBatch>,
    interval_ms: Option<u64>,
    max_bytes: Option<usize>,
) -> Result<()> {
    let collector = state.raw_data_collector.clone();
    let interval = Duration::from_millis(interval_ms.unwrap_or(16));
    let max_n = max_bytes.unwrap_or(65536);
    let channel_id = on_event.id();

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.raw_data_tasks.lock().insert(channel_id, cancel_tx);

    tokio::spawn(async move {
        crate::state::rawdata_loop(collector, on_event, interval, max_n, cancel_rx).await;
    });
    Ok(())
}

/// 取消订阅原始数据
#[tauri::command]
pub async fn unsubscribe_rawdata(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    if let Some(tx) = state.raw_data_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

/// 清空原始数据收集器
#[tauri::command]
pub async fn clear_raw_data_collector(state: State<'_, AppState>) -> Result<()> {
    state.raw_data_collector.lock().clear();
    Ok(())
}

// ============ CAN 帧相关 ============

/// 发送 CAN 帧
///
/// 通过当前协议引擎的 encode_can 编码为字节, 再通过传输层发送。
/// 若当前协议不是 CAN 协议 (encode_can 返回空), 直接返回 Ok。
#[tauri::command]
pub async fn send_can_frame(state: State<'_, AppState>, frame: CanFrame) -> Result<()> {
    let data = state.protocol.lock().encode_can(&frame);
    if data.is_empty() {
        return Ok(()); // 非 CAN 协议, 忽略
    }
    let manager = state.transport.lock().await;
    manager.send(&data).await
}

/// 订阅 CAN 帧推送 — 通过 Channel 定期推送最近 N 帧
///
/// - interval_ms: 推送间隔 (默认 100ms)
/// - max_frames: 单次推送最大帧数 (默认 500)
///
/// 取消方式: 前端调用 unsubscribe_can_frames(channel_id)
#[tauri::command]
pub async fn subscribe_can_frames(
    state: State<'_, AppState>,
    on_event: Channel<CanFrameBatch>,
    interval_ms: Option<u64>,
    max_frames: Option<usize>,
) -> Result<()> {
    let buffer = state.can_buffer.clone();
    let interval = Duration::from_millis(interval_ms.unwrap_or(100));
    let max_n = max_frames.unwrap_or(500);
    let channel_id = on_event.id();

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.can_tasks.lock().insert(channel_id, cancel_tx);

    tokio::spawn(async move {
        crate::state::can_frames_loop(buffer, on_event, interval, max_n, cancel_rx).await;
    });
    Ok(())
}

/// 取消订阅 CAN 帧
#[tauri::command]
pub async fn unsubscribe_can_frames(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    if let Some(tx) = state.can_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

/// 同步查询: 获取最近 N 个 CAN 帧
#[tauri::command]
pub async fn get_recent_can_frames(
    state: State<'_, AppState>,
    count: usize,
) -> Result<Vec<CanFrame>> {
    Ok(state.can_buffer.lock().get_recent(count))
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
    vofa_next_transport::candle::list_devices()
}

// ============ 逻辑分析仪命令 ============

/// 订阅逻辑采样数据 — 通过 Tauri Channel 周期性推送
#[tauri::command]
pub async fn subscribe_logic_samples(
    state: State<'_, AppState>,
    on_event: Channel<LogicSampleBatch>,
    interval_ms: Option<u64>,
    max_samples: Option<usize>,
) -> Result<()> {
    let logic_buffer = state.logic_buffer.clone();
    let interval = Duration::from_millis(interval_ms.unwrap_or(100));
    let max_n = max_samples.unwrap_or(500);
    let channel_id = on_event.id();

    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.logic_tasks.lock().insert(channel_id, cancel_tx);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        log::info!("逻辑采样订阅已启动, channel_id={}", channel_id);
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    log::info!("逻辑采样订阅被取消, channel_id={}", channel_id);
                    break;
                }
                _ = ticker.tick() => {
                    let samples = {
                        let buf = logic_buffer.lock();
                        buf.get_recent(max_n)
                    };
                    if samples.is_empty() {
                        continue;
                    }
                    if on_event.send(LogicSampleBatch { samples }).is_err() {
                        break;
                    }
                }
            }
        }
    });

    Ok(())
}

/// 取消订阅逻辑采样
#[tauri::command]
pub async fn unsubscribe_logic_samples(
    state: State<'_, AppState>,
    channel_id: u32,
) -> Result<()> {
    if let Some(tx) = state.logic_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

/// 同步查询: 获取最近 N 个逻辑采样
#[tauri::command]
pub async fn get_recent_logic_samples(
    state: State<'_, AppState>,
    count: usize,
) -> Result<LogicSampleBatch> {
    let samples = state.logic_buffer.lock().get_recent(count);
    Ok(LogicSampleBatch { samples })
}

/// 清空逻辑采样缓冲区
#[tauri::command]
pub async fn clear_logic_buffer(state: State<'_, AppState>) -> Result<()> {
    state.logic_buffer.lock().clear();
    Ok(())
}

/// 获取逻辑采样缓冲区当前数量
#[tauri::command]
pub async fn get_logic_buffer_info(state: State<'_, AppState>) -> Result<usize> {
    Ok(state.logic_buffer.lock().len())
}

/// 订阅解码事件 — 通过 Tauri Channel 周期性推送
#[tauri::command]
pub async fn subscribe_decoded_events(
    state: State<'_, AppState>,
    on_event: Channel<DecodedEventBatch>,
    interval_ms: Option<u64>,
    max_events: Option<usize>,
) -> Result<()> {
    let decoded_buffer = state.decoded_buffer.clone();
    let interval = Duration::from_millis(interval_ms.unwrap_or(100));
    let max_n = max_events.unwrap_or(200);
    let channel_id = on_event.id();

    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.decoded_tasks.lock().insert(channel_id, cancel_tx);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        log::info!("解码事件订阅已启动, channel_id={}", channel_id);
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    log::info!("解码事件订阅被取消, channel_id={}", channel_id);
                    break;
                }
                _ = ticker.tick() => {
                    let events = {
                        let buf = decoded_buffer.lock();
                        buf.get_recent(max_n)
                    };
                    if events.is_empty() {
                        continue;
                    }
                    if on_event.send(DecodedEventBatch { events }).is_err() {
                        break;
                    }
                }
            }
        }
    });

    Ok(())
}

/// 取消订阅解码事件
#[tauri::command]
pub async fn unsubscribe_decoded_events(
    state: State<'_, AppState>,
    channel_id: u32,
) -> Result<()> {
    if let Some(tx) = state.decoded_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

/// 同步查询: 获取最近 N 个解码事件
#[tauri::command]
pub async fn get_recent_decoded_events(
    state: State<'_, AppState>,
    count: usize,
) -> Result<DecodedEventBatch> {
    let events = state.decoded_buffer.lock().get_recent(count);
    Ok(DecodedEventBatch { events })
}

/// 清空解码事件缓冲区
#[tauri::command]
pub async fn clear_decoded_buffer(state: State<'_, AppState>) -> Result<()> {
    state.decoded_buffer.lock().clear();
    Ok(())
}

/// 获取解码事件缓冲区当前数量
#[tauri::command]
pub async fn get_decoded_buffer_info(state: State<'_, AppState>) -> Result<usize> {
    Ok(state.decoded_buffer.lock().len())
}

// ============ CAN 负载分析 ============

/// 从当前 TransportConfig 提取 CAN 波特率 (bps)
///
/// 仅 Slcan / CandleLight 配置携带 CAN 波特率; 其他传输方式返回 None。
async fn extract_can_bitrate_from_transport(state: &AppState) -> Option<u32> {
    let manager = state.transport.lock().await;
    match manager.config() {
        Some(vofa_next_core::TransportConfig::Slcan(s)) => Some(s.can_bitrate.bps()),
        Some(vofa_next_core::TransportConfig::CandleLight(c)) => Some(c.can_bitrate.bps()),
        _ => None,
    }
}

/// 计算有效 CAN 波特率 (bps)
///
/// - 若 `override_bps` 为 Some(n) 且 n > 0, 使用 n (手动覆盖)
/// - 否则尝试从当前 TransportConfig 读取
/// - 都没有则返回 500_000 (默认值, 避免前端传 0 导致除零)
async fn resolve_can_bitrate(state: &AppState, override_bps: Option<u32>) -> u32 {
    if let Some(bps) = override_bps {
        if bps > 0 {
            return bps;
        }
    }
    extract_can_bitrate_from_transport(state).await.unwrap_or(500_000)
}

/// 获取 CAN 负载统计快照
///
/// `bitrate_bps`: 可选手动覆盖波特率; None/0 = 自动从 TransportConfig 读取
#[tauri::command]
pub async fn get_can_load_stats(
    state: State<'_, AppState>,
    bitrate_bps: Option<u32>,
) -> Result<CanLoadSnapshot> {
    let bitrate = resolve_can_bitrate(&state, bitrate_bps).await;
    let stats = state.can_load_stats.lock();
    Ok(stats.snapshot(bitrate))
}

/// 设置 CAN 负载统计滑动窗口大小 (微秒)
///
/// 例如 1_000_000 = 1 秒, 100_000 = 100ms
#[tauri::command]
pub async fn set_can_load_window(state: State<'_, AppState>, window_us: u64) -> Result<()> {
    state.can_load_stats.lock().set_window_us(window_us);
    Ok(())
}

/// 清空 CAN 负载统计
#[tauri::command]
pub async fn clear_can_load_stats(state: State<'_, AppState>) -> Result<()> {
    state.can_load_stats.lock().clear();
    Ok(())
}

/// 订阅 CAN 负载统计推送 — 周期性推送 CanLoadSnapshot (含 history 时序数据)
///
/// - `interval_ms`: 推送间隔 (默认 500ms)
/// - `bitrate_bps`: 可选手动覆盖波特率; None/0 = 自动从 TransportConfig 读取
/// - 每次推送前会调用 `sample_history(bitrate, now_us)` 记录一个采样点
///
/// 取消方式: 前端调用 unsubscribe_can_load(channel_id)
///
/// **注意**: bitrate 在订阅时一次性解析, 后续不会跟随 TransportConfig 变化;
/// 若前端修改了 CAN 波特率, 需重新订阅。
#[tauri::command]
pub async fn subscribe_can_load(
    state: State<'_, AppState>,
    on_event: Channel<CanLoadSnapshot>,
    interval_ms: Option<u64>,
    bitrate_bps: Option<u32>,
) -> Result<()> {
    let stats = state.can_load_stats.clone();
    let interval = Duration::from_millis(interval_ms.unwrap_or(500));
    let channel_id = on_event.id();

    let bitrate = resolve_can_bitrate(&state, bitrate_bps).await;

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    state.can_load_tasks.lock().insert(channel_id, cancel_tx);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        log::info!(
            "CAN 负载订阅已启动, channel_id={}, 间隔={}ms, bitrate={}bps",
            channel_id,
            interval.as_millis(),
            bitrate
        );
        let mut cancel_rx = cancel_rx;
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    log::info!("CAN 负载订阅被取消, channel_id={}", channel_id);
                    break;
                }
                _ = ticker.tick() => {
                    let snap = {
                        let mut s = stats.lock();
                        s.sample_history(bitrate, now_us());
                        s.snapshot(bitrate)
                    };
                    if on_event.send(snap).is_err() {
                        log::info!("CAN 负载订阅通道已关闭, channel_id={}", channel_id);
                        break;
                    }
                }
            }
        }
    });

    Ok(())
}

/// 取消订阅 CAN 负载统计
#[tauri::command]
pub async fn unsubscribe_can_load(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    if let Some(tx) = state.can_load_tasks.lock().remove(&channel_id) {
        let _ = tx.send(());
    }
    Ok(())
}

// ============ 协议输入解析 ============

/// 帧解码器手动测试结果 (与前端 FrameDecoderManualResult 对应)
#[derive(Debug, Clone, serde::Serialize)]
pub struct FrameDecoderManualResult {
    /// 端口名 → 值 (来自 field/bitfield/length/id 块)
    pub outputs: std::collections::HashMap<String, f32>,
    /// 校验是否通过
    pub valid: bool,
    /// 本帧消耗的字节数 (header + 所有 blocks)
    pub consumed_bytes: usize,
    /// 错误信息 (Hex 解析失败 / 帧头未找到 / 帧不完整等)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 解析用户输入字符串为帧 (用于 FrameDecoder 手动测试模式)
///
/// 使用 blocks 配置创建临时 FrameParser, 调用 parse_once_with_consumed
/// 返回 outputs + valid + consumed_bytes + 可选 error
#[tauri::command]
pub async fn parse_frame_decoder_input(
    blocks: Vec<vofa_next_nodes::DecoderBlockDef>,
    input: String,
    format: InputFormat,
    enable_valid: bool,
    enable_frame_count: bool,
    enable_last_timestamp: bool,
    enable_fps: bool,
) -> Result<FrameDecoderManualResult> {
    use vofa_next_nodes::FrameParser;
    use vofa_next_protocol::engine::{detect_format, parse_ascii, parse_hex};

    // 1. 解析输入字符串为字节
    let actual_format = match format {
        InputFormat::Auto => detect_format(&input),
        f => f,
    };
    let bytes = match actual_format {
        InputFormat::Hex => match parse_hex(&input) {
            Ok(b) => b,
            Err(e) => {
                return Ok(FrameDecoderManualResult {
                    outputs: std::collections::HashMap::new(),
                    valid: false,
                    consumed_bytes: 0,
                    error: Some(e),
                });
            }
        },
        InputFormat::Ascii => parse_ascii(&input),
        InputFormat::Auto => unreachable!(),
    };

    // 2. 创建临时 FrameParser (无状态, 仅用于一次性解析)
    let parser = FrameParser::new(
        blocks,
        enable_valid,
        enable_frame_count,
        enable_last_timestamp,
        enable_fps,
    );

    // 3. 解析一帧 — 使用当前系统时间作为时间戳 (微秒)
    let now_us = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);

    match parser.parse_once_with_consumed(&bytes, now_us) {
        Some((frame, consumed)) => Ok(FrameDecoderManualResult {
            outputs: frame.outputs,
            valid: frame.valid,
            consumed_bytes: consumed,
            error: None,
        }),
        None => Ok(FrameDecoderManualResult {
            outputs: std::collections::HashMap::new(),
            valid: false,
            consumed_bytes: 0,
            error: Some("无法解析: 未找到帧头或帧不完整".to_string()),
        }),
    }
}

// ============ 工具函数 ============

/// 获取当前 CAN 波特率 (从 TransportConfig 提取, 用于前端 UI 默认值)
///
/// 返回 (bps, source) — source 描述来源 ("slcan" / "candle" / "default")
#[tauri::command]
pub async fn get_current_can_bitrate(state: State<'_, AppState>) -> Result<(u32, String)> {
    let manager = state.transport.lock().await;
    if let Some(cfg) = manager.config() {
        match cfg {
            vofa_next_core::TransportConfig::Slcan(s) => {
                return Ok((s.can_bitrate.bps(), "slcan".to_string()));
            }
            vofa_next_core::TransportConfig::CandleLight(c) => {
                return Ok((c.can_bitrate.bps(), "candle".to_string()));
            }
            _ => {}
        }
    }
    Ok((500_000, "default".to_string()))
}

/// 导出 CAN 负载统计为 CSV 文件
///
/// 自动保存到用户下载目录, 文件名格式: `vofa-can-load-YYYYMMDD-HHMMSS.csv`
///
/// CSV 结构:
/// - 元信息头 (# 开头): 导出时间 / 波特率 / 窗口大小
/// - Section: History — 时间戳, 负载率, 帧率
/// - Section: Per-ID — ID, 扩展帧, 帧数, 总位数, 总字节数
/// - Section: Per-ID History — ID, 扩展帧, 时间戳, 负载率
///
/// 返回完整文件路径
#[tauri::command]
pub async fn export_can_load_csv(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    bitrate_bps: Option<u32>,
) -> Result<String> {
    use std::io::Write;

    let bitrate = resolve_can_bitrate(&state, bitrate_bps).await;
    let snap = state.can_load_stats.lock().snapshot(bitrate);

    // 生成时间戳 (本地时间, 不依赖 chrono)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (yyyy, mm, dd, hh, min, ss) = secs_to_local_components(now);
    let timestamp_str = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", yyyy, mm, dd, hh, min, ss);
    let filename = format!("vofa-can-load-{:04}{:02}{:02}-{:02}{:02}{:02}.csv", yyyy, mm, dd, hh, min, ss);

    let csv = format_can_load_csv(&snap, bitrate, &timestamp_str);

    // 选择保存路径: 优先 Downloads, 失败则用当前目录
    let path = match app.path().download_dir() {
        Ok(d) => d.join(&filename),
        Err(_) => std::env::current_dir()
            .map(|d| d.join(&filename))
            .map_err(|e| vofa_next_core::Error::Config(format!("无法确定下载目录: {}", e)))?,
    };

    let mut file = std::fs::File::create(&path)?;
    file.write_all(csv.as_bytes())?;

    log::info!("CAN 负载 CSV 已导出: {}", path.display());
    Ok(path.to_string_lossy().to_string())
}

/// 将 UNIX 秒数转换为本地时间组件 (年月日时分秒)
/// 简化实现, 不依赖 chrono — 假设本地时区为系统设置的时区
fn secs_to_local_components(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    // 用 libc localtime_r 获取本地时间 (跨平台)
    #[cfg(unix)]
    {
        use std::os::raw::*;
        extern "C" {
            fn localtime_r(time: *const c_long, result: *mut libc_tm) -> *mut libc_tm;
        }
        #[repr(C)]
        struct libc_tm {
            tm_sec: c_int,
            tm_min: c_int,
            tm_hour: c_int,
            tm_mday: c_int,
            tm_mon: c_int,
            tm_year: c_int,
            tm_wday: c_int,
            tm_yday: c_int,
            tm_isdst: c_int,
            tm_gmtoff: c_long,
            tm_zone: *const c_char,
        }
        let t: c_long = secs as c_long;
        let mut tm = libc_tm {
            tm_sec: 0,
            tm_min: 0,
            tm_hour: 0,
            tm_mday: 0,
            tm_mon: 0,
            tm_year: 0,
            tm_wday: 0,
            tm_yday: 0,
            tm_isdst: 0,
            tm_gmtoff: 0,
            tm_zone: std::ptr::null(),
        };
        unsafe {
            localtime_r(&t, &mut tm);
            (
                (tm.tm_year + 1900) as u32,
                (tm.tm_mon + 1) as u32,
                tm.tm_mday as u32,
                tm.tm_hour as u32,
                tm.tm_min as u32,
                tm.tm_sec as u32,
            )
        }
    }
    #[cfg(not(unix))]
    {
        // 非 Unix 简化回退: 用 UTC
        let days = secs / 86400;
        let sec_of_day = secs % 86400;
        let hh = (sec_of_day / 3600) as u32;
        let min = ((sec_of_day % 3600) / 60) as u32;
        let ss = (sec_of_day % 60) as u32;
        // 简化日期计算 (从 1970-01-01 开始)
        let mut year = 1970u32;
        let mut remaining_days = days as u32;
        loop {
            let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
            let days_in_year = if leap { 366 } else { 365 };
            if remaining_days < days_in_year {
                break;
            }
            remaining_days -= days_in_year;
            year += 1;
        }
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_per_month = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut month = 1u32;
        for &dim in &days_per_month {
            if remaining_days < dim {
                break;
            }
            remaining_days -= dim;
            month += 1;
        }
        (year, month, remaining_days + 1, hh, min, ss)
    }
}

/// 格式化 CanLoadSnapshot 为 CSV 字符串
fn format_can_load_csv(snap: &CanLoadSnapshot, bitrate: u32, export_time: &str) -> String {
    let mut s = String::with_capacity(8192);
    // 元信息头
    s.push_str(&format!("# VOFA-Next CAN Load Stats Export\n"));
    s.push_str(&format!("# Export Time: {}\n", export_time));
    s.push_str(&format!("# Bitrate: {} bps\n", bitrate));
    s.push_str(&format!(
        "# Window: {} us ({})\n",
        snap.window_us,
        if snap.window_us >= 1_000_000 {
            format!("{}s", snap.window_us / 1_000_000)
        } else {
            format!("{}ms", snap.window_us / 1000)
        }
    ));
    s.push_str(&format!(
        "# Summary: frames={}, total_bits={}, total_bytes={}, load_ratio={:.4}\n",
        snap.frame_count, snap.total_bits, snap.total_bytes, snap.load_ratio
    ));
    s.push('\n');

    // Section: History
    s.push_str("# Section: History\n");
    s.push_str("timestamp_us,load_ratio,fps\n");
    for p in &snap.history {
        s.push_str(&format!("{},{:.6},{:.2}\n", p.timestamp, p.load_ratio, p.fps));
    }
    s.push('\n');

    // Section: Per-ID
    s.push_str("# Section: Per-ID\n");
    s.push_str("id_hex,extended,frame_count,total_bits,total_bytes\n");
    for id_stat in &snap.per_id {
        s.push_str(&format!(
            "0x{:X},{},{},{},{}\n",
            id_stat.id,
            id_stat.extended,
            id_stat.frame_count,
            id_stat.total_bits,
            id_stat.total_bytes
        ));
    }
    s.push('\n');

    // Section: Per-ID History
    s.push_str("# Section: Per-ID History\n");
    s.push_str("id_hex,extended,timestamp_us,load_ratio\n");
    for h in &snap.per_id_history {
        for p in &h.history {
            s.push_str(&format!(
                "0x{:X},{},{},{:.6}\n",
                h.id, h.extended, p.timestamp, p.load_ratio
            ));
        }
    }

    s
}

fn now_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}
