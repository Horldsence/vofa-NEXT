use crate::notify;
use crate::state::{data_loop, AppState, CustomInputBatch, GraphOutputSnapshot, SpectrumBatch};
use std::time::Duration;
use tauri::{ipc::Channel, AppHandle, Emitter, State};
use vofa_next_buffer::{graph::Edge, RawDataBatch, WaveformWindow};
use vofa_next_core::{
    CanFrame, CanFrameBatch, CandleDeviceInfo, ConnectionState, DecodedEventBatch,
    LogicSampleBatch, PortInfo, ProtocolConfig, Result, TransportConfig,
    TransportStats, WidgetBinding,
};
use vofa_next_nodes::NodeDef;
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
        let logic_buffer = state.logic_buffer.clone();
        let decoded_buffer = state.decoded_buffer.clone();
        tokio::spawn(async move {
            data_loop(app, rx, protocol, buffer, eval_state, raw_data_collector, can_buffer, logic_buffer, decoded_buffer).await;
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
