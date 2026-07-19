use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tauri::ipc::Channel;
use tokio::sync::{mpsc, oneshot};
use vofa_next_buffer::{DataBuffer, RawDataCollector};
use vofa_next_core::{
    CanBuffer, CanFrameBatch, ConnectionState, DataFrame, DecodedBuffer,
    DecodedEventBatch, LogicBuffer, LogicSampleBatch, ProtocolConfig, TransportStats,
};
use vofa_next_dsp::{DigitalFilter, SpectrumAnalyzer, SpectrumResult};
use vofa_next_nodes::CompiledGraph;
use vofa_next_protocol::ProtocolEngine;
use vofa_next_transport::TransportManager;

/// 单个图输出快照 — 通过 Channel 推送到前端
///
/// values: widgetId -> portId -> value
/// 包含 ChannelSource/Input/Math/Custom/Filter 节点的输出
/// 前端通过 edges 自行解析 Sink 节点的输入 (上游 widgetId + sourceHandle)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphOutputSnapshot {
    /// 自增计数器, 前端可用于去重/丢弃过期帧
    pub tick: u64,
    /// widgetId -> portId -> value
    pub values: HashMap<String, HashMap<String, f32>>,
}

/// Custom widget 输入批次 — 后端推送到前端 iframe
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CustomInputBatch {
    /// custom widget id -> input port id -> value
    pub inputs: HashMap<String, HashMap<String, f32>>,
}

/// 频谱分析结果批次 — 后端推送到前端 SpectrumChart
///
/// 30 FPS 推送, key = SpectrumSink widget id, value = 最新一次 FFT 结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpectrumBatch {
    /// sink widget id -> 频谱结果
    pub spectra: HashMap<String, SpectrumResult>,
}

/// 节点图评估所需的共享状态 (从 AppState 抽取, 供 data_loop 使用)
///
/// 设计动机: Tauri 2 的 State<'_, T> 内部是 &Arc<T> 但不暴露 Arc,
/// 我们也无法在 manage() 时包装 AppState 成 Arc<AppState> (因为 tauri::manage
/// 内部已用 Arc)。因此把 data_loop 需要的字段单独打包为 Arc, 从 AppState 克隆。
pub struct GraphEvalState {
    pub graphs: Arc<Mutex<HashMap<String, CompiledGraph>>>,
    pub input_values: Arc<Mutex<HashMap<String, f32>>>,
    pub custom_outputs: Arc<Mutex<HashMap<String, HashMap<String, f32>>>>,
    pub output_snapshot: Arc<Mutex<GraphOutputSnapshot>>,
    pub output_subscribers: Arc<Mutex<Vec<Channel<GraphOutputSnapshot>>>>,
    pub custom_input_subscribers: Arc<Mutex<Vec<Channel<CustomInputBatch>>>>,
    /// Filter 节点状态 (跨帧持久化, 逐点滤波)
    /// key: Filter widget id, value: DigitalFilter (含 FIR 延迟线 / IIR biquad 状态)
    pub filter_states: Arc<Mutex<HashMap<String, DigitalFilter>>>,
    /// SpectrumSink 节点对应的频谱分析器
    /// key: SpectrumSink widget id, value: SpectrumAnalyzer (含滑动窗口)
    /// 由 spectrum_ticker 在每 tick 开头与 graphs 同步 (增删)
    pub spectrum_analyzers: Arc<Mutex<HashMap<String, SpectrumAnalyzer>>>,
    /// 最新一次 FFT 结果 (供 30 FPS spectrum_ticker 推送)
    /// key: SpectrumSink widget id, value: SpectrumResult
    pub spectrum_snapshot: Arc<Mutex<HashMap<String, SpectrumResult>>>,
    /// 频谱订阅者 (30 FPS 推送 SpectrumBatch)
    pub spectrum_subscribers: Arc<Mutex<Vec<Channel<SpectrumBatch>>>>,
}

/// 应用全局状态
pub struct AppState {
    /// 传输管理器 (async mutex, 因为 open/send 是异步的)
    pub transport: tokio::sync::Mutex<TransportManager>,
    /// 协议引擎 (sync mutex, feed/encode 是同步的)
    pub protocol: Arc<Mutex<Box<dyn ProtocolEngine>>>,
    /// 当前协议配置
    pub protocol_config: Mutex<ProtocolConfig>,
    /// 多通道数据缓冲区
    pub buffer: Arc<Mutex<DataBuffer>>,
    /// 节点图 — 按 tab_id 索引 (每个 tab 独立编译图)
    pub graphs: Arc<Mutex<HashMap<String, CompiledGraph>>>,
    /// 输入控件当前值 (Knob/Slider/Button/Radio/Checkbox)
    /// key: widget_id, value: 当前值
    /// 由前端 invoke('set_input_value') 更新
    pub input_values: Arc<Mutex<HashMap<String, f32>>>,
    /// Custom widget 回传输出
    /// key: widget_id, value: portId -> value
    /// 由前端 invoke('submit_custom_output') 更新
    pub custom_outputs: Arc<Mutex<HashMap<String, HashMap<String, f32>>>>,
    /// 最新一帧的图输出快照 (供 60 FPS ticker 推送)
    pub output_snapshot: Arc<Mutex<GraphOutputSnapshot>>,
    /// 图输出订阅者 (60 FPS 推送)
    pub output_subscribers: Arc<Mutex<Vec<Channel<GraphOutputSnapshot>>>>,
    /// Custom 输入订阅者 (30 FPS 推送到前端 iframe)
    pub custom_input_subscribers: Arc<Mutex<Vec<Channel<CustomInputBatch>>>>,
    /// Filter 节点状态 (跨帧持久化)
    pub filter_states: Arc<Mutex<HashMap<String, DigitalFilter>>>,
    /// SpectrumSink 节点对应的频谱分析器
    pub spectrum_analyzers: Arc<Mutex<HashMap<String, SpectrumAnalyzer>>>,
    /// 最新一次 FFT 结果快照
    pub spectrum_snapshot: Arc<Mutex<HashMap<String, SpectrumResult>>>,
    /// 频谱订阅者 (30 FPS 推送)
    pub spectrum_subscribers: Arc<Mutex<Vec<Channel<SpectrumBatch>>>>,
    /// 波形订阅任务的取消句柄 — key: channel_id, value: oneshot sender
    /// 前端调用 unsubscribe_waveform 时, 通过 channel_id 取出 sender 发送取消信号,
    /// 让 tokio::spawn 的 task 优雅退出, 避免向已关闭的 channel send 产生警告。
    pub waveform_tasks: Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>,
    /// 原始数据收集器
    pub raw_data_collector: Arc<Mutex<RawDataCollector>>,
    /// 原始数据订阅任务的取消句柄
    pub raw_data_tasks: Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>,
    /// CAN 帧缓冲区
    pub can_buffer: Arc<Mutex<CanBuffer>>,
    /// CAN 订阅任务的取消句柄 — key: channel_id
    pub can_tasks: Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>,
    /// 逻辑采样缓冲区
    pub logic_buffer: Arc<Mutex<LogicBuffer>>,
    /// 解码事件缓冲区
    pub decoded_buffer: Arc<Mutex<DecodedBuffer>>,
    /// 逻辑采样订阅任务的取消句柄
    pub logic_tasks: Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>,
    /// 解码事件订阅任务的取消句柄
    pub decoded_tasks: Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            transport: tokio::sync::Mutex::new(TransportManager::new()),
            protocol: Arc::new(Mutex::new(vofa_next_protocol::create_engine(&ProtocolConfig::default()))),
            protocol_config: Mutex::new(ProtocolConfig::default()),
            buffer: Arc::new(Mutex::new(DataBuffer::new(100_000, 4))),
            graphs: Arc::new(Mutex::new(HashMap::new())),
            input_values: Arc::new(Mutex::new(HashMap::new())),
            custom_outputs: Arc::new(Mutex::new(HashMap::new())),
            output_snapshot: Arc::new(Mutex::new(GraphOutputSnapshot {
                tick: 0,
                values: HashMap::new(),
            })),
            output_subscribers: Arc::new(Mutex::new(Vec::new())),
            custom_input_subscribers: Arc::new(Mutex::new(Vec::new())),
            filter_states: Arc::new(Mutex::new(HashMap::new())),
            spectrum_analyzers: Arc::new(Mutex::new(HashMap::new())),
            spectrum_snapshot: Arc::new(Mutex::new(HashMap::new())),
            spectrum_subscribers: Arc::new(Mutex::new(Vec::new())),
            waveform_tasks: Arc::new(Mutex::new(HashMap::new())),
            raw_data_collector: Arc::new(Mutex::new(RawDataCollector::new())),
            raw_data_tasks: Arc::new(Mutex::new(HashMap::new())),
            can_buffer: Arc::new(Mutex::new(CanBuffer::new(100_000))),
            can_tasks: Arc::new(Mutex::new(HashMap::new())),
            logic_buffer: Arc::new(Mutex::new(LogicBuffer::new(20_000))),
            decoded_buffer: Arc::new(Mutex::new(DecodedBuffer::new(10_000))),
            logic_tasks: Arc::new(Mutex::new(HashMap::new())),
            decoded_tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 抽取图评估所需的 Arc 字段 (供 data_loop 持有)
    pub fn eval_state(&self) -> GraphEvalState {
        GraphEvalState {
            graphs: self.graphs.clone(),
            input_values: self.input_values.clone(),
            custom_outputs: self.custom_outputs.clone(),
            output_snapshot: self.output_snapshot.clone(),
            output_subscribers: self.output_subscribers.clone(),
            custom_input_subscribers: self.custom_input_subscribers.clone(),
            filter_states: self.filter_states.clone(),
            spectrum_analyzers: self.spectrum_analyzers.clone(),
            spectrum_snapshot: self.spectrum_snapshot.clone(),
            spectrum_subscribers: self.spectrum_subscribers.clone(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// 统计节流间隔 (毫秒) — 避免高波特率下每包都 emit 统计
const STATS_THROTTLE_MS: u128 = 100;

/// 评估所有图 (静态函数版本, 供 GraphEvalState 使用)
///
/// 步骤:
/// 1. 对每个图调用 evaluate (传入 filter_states, 逐点滤波跨帧持久化)
/// 2. 合并所有图输出到 output_snapshot
/// 3. 遍历所有图的 SpectrumSink, 从 output_snapshot 取输入值, push 到对应 analyzer
fn evaluate_all_graphs_with(eval_state: &GraphEvalState, frame: &DataFrame) {
    let input_values = eval_state.input_values.lock().clone();
    let custom_outputs = eval_state.custom_outputs.lock().clone();
    let graphs = eval_state.graphs.lock();
    let mut filter_states = eval_state.filter_states.lock();

    let mut combined: HashMap<String, HashMap<String, f32>> = HashMap::new();
    for (_, graph) in graphs.iter() {
        let out = graph.evaluate(frame, &input_values, &custom_outputs, &mut filter_states);
        for (k, v) in out {
            combined.insert(k, v);
        }
    }

    // 更新 output_snapshot (供 60 FPS ticker 推送)
    {
        let mut snap = eval_state.output_snapshot.lock();
        snap.tick = snap.tick.wrapping_add(1);
        snap.values = combined.clone();
    }

    // 收集 SpectrumSink 输入值, push 到对应 analyzer 的滑动窗口
    // analyzer 的创建/删除由 spectrum_ticker 在每 tick 开头与 graphs 同步
    let mut analyzers = eval_state.spectrum_analyzers.lock();
    if !analyzers.is_empty() {
        for (_, graph) in graphs.iter() {
            let spectrum_inputs = graph.collect_spectrum_inputs(&combined);
            for (sink_id, value) in spectrum_inputs {
                if let Some(analyzer) = analyzers.get_mut(&sink_id) {
                    analyzer.push(value);
                }
            }
        }
    }
}

/// 从 output_snapshot 收集派生值, push 到 buffer 的 derived_buffers
///
/// 遍历所有 graph 的 edges, 对每条 edge:
///   若 source 在 output_snapshot 中 (即 source 是有输出的节点: Math/Input/Custom/ChannelSource):
///     取 snapshot[source][sourceHandle], push 到 buffer.derived_buffers[(target, source)]
///
/// **时间对齐**: 本函数在每帧 evaluate_all_graphs_with 后调用,
/// 与 push_frame 同步, 保证 derived[i] 与 timestamps[i] 对齐。
fn push_derived_from_snapshot(eval_state: &GraphEvalState, buffer: &mut DataBuffer) {
    let snap = eval_state.output_snapshot.lock();
    let graphs = eval_state.graphs.lock();
    for (_, graph) in graphs.iter() {
        for e in graph.edges() {
            // 只对有输出的 source (ChannelSource/Input/Math/Custom) 收集派生值
            if let Some(src_out) = snap.values.get(&e.source) {
                if let Some(&val) = src_out.get(&e.source_handle) {
                    buffer.push_derived(&e.target, &e.source, val);
                }
            }
        }
    }
}

/// 数据循环 — 快速消费传输层 broadcast, 转发到解析 task
///
/// 架构 (节点图后端化重构):
/// - data_loop (本函数): 只做 recv + mpsc.send, 最快消费 broadcast 避免 Lagged
/// - parse_task: 从 mpsc 收数据, 做 协议解析 + buffer.push + 图评估 + 批量 emit
///   - 批量 emit: `transport:frames` (数组) 替代每帧一次 `transport:frame`
///   - 统计节流: STATS_THROTTLE_MS 内累积, 一次性 emit
///   - 图评估: 调用 evaluate_all_graphs_with 实时计算所有节点输出
///     结果存入 output_snapshot, 由独立的 60 FPS ticker task 推送到前端
pub async fn data_loop(
    app: AppHandle,
    mut rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
    protocol: Arc<Mutex<Box<dyn ProtocolEngine>>>,
    buffer: Arc<Mutex<DataBuffer>>,
    eval_state: GraphEvalState,
    raw_data_collector: Arc<Mutex<RawDataCollector>>,
    can_buffer: Arc<Mutex<CanBuffer>>,
    logic_buffer: Arc<Mutex<LogicBuffer>>,
    decoded_buffer: Arc<Mutex<DecodedBuffer>>,
) {
    log::info!("数据循环已启动");

    // 解析 task 用的 mpsc (大容量缓冲, 避免反压到 data_loop)
    let (parse_tx, mut parse_rx) = mpsc::channel::<Vec<u8>>(2048);
    let app2 = app.clone();
    let proto2 = protocol.clone();
    let buf2 = buffer.clone();
    let eval2 = eval_state;
    let raw_collector2 = raw_data_collector;
    let can_buffer2 = can_buffer;
    let logic_buffer2 = logic_buffer;
    let decoded_buffer2 = decoded_buffer;

    let parse_task = tokio::spawn(async move {
        let mut detection_notified = false;
        let mut frame_batch: Vec<DataFrame> = Vec::with_capacity(128);
        let mut last_stats = Instant::now();
        let mut acc_bytes: u64 = 0;
        let mut acc_frames: u64 = 0;

        while let Some(data) = parse_rx.recv().await {
            // 1. 收集原始数据 (通过 Channel 周期性推送, 替代每包 emit)
            raw_collector2.lock().push_chunk(now_us(), &data);

            // 2. 协议解析
            let frames = proto2.lock().feed(&data);
            acc_bytes += data.len() as u64;
            acc_frames += frames.len() as u64;

            // 2.x CAN 帧解析 (slcan/candleLight) — 非 CAN 协议返回空 Vec
            let can_frames = proto2.lock().feed_can(&data);
            if !can_frames.is_empty() {
                // push 到 can_buffer
                {
                    let mut buf = can_buffer2.lock();
                    for f in &can_frames {
                        buf.push(f.clone());
                    }
                }
                // emit 批次事件 (实时推送到前端, 供监听 transport:can-frames 的组件使用)
                let _ = app2.emit(
                    "transport:can-frames",
                    &CanFrameBatch { frames: can_frames.clone() },
                );
            }

            // 2.x 逻辑采样 + 解码事件 (LogicDecoder 协议)
            let logic_samples = proto2.lock().feed_logic(&data);
            if !logic_samples.is_empty() {
                {
                    let mut lb = logic_buffer2.lock();
                    for s in &logic_samples {
                        lb.push(s.clone());
                    }
                }
                let _ = app2.emit(
                    "transport:logic-samples",
                    &LogicSampleBatch { samples: logic_samples },
                );
            }
            let decoded_events = proto2.lock().feed_decoded(&data);
            if !decoded_events.is_empty() {
                {
                    let mut db = decoded_buffer2.lock();
                    for e in &decoded_events {
                        db.push(e.clone());
                    }
                }
                let _ = app2.emit(
                    "transport:decoded-events",
                    &DecodedEventBatch { events: decoded_events },
                );
            }

            // 2.1 自动通道检测通知 (一次性)
            if !detection_notified {
                let p = proto2.lock();
                if p.is_auto_mode() {
                    if let Some(n) = p.detected_channels() {
                        crate::notify::channels_detected(&app2, n);
                        detection_notified = true;
                    }
                }
            }

            // 3. 推入缓冲区 + 评估节点图 + 收集派生值 (每帧实时计算)
            //    三步必须在同一帧内顺序执行, 保证 derived 与 timestamps 对齐
            if !frames.is_empty() {
                for f in &frames {
                    // 3.1 push 原始帧到 buffer
                    {
                        let mut buf = buf2.lock();
                        buf.push_frame(f);
                    }
                    // 3.2 评估所有 tab 的图, 更新 output_snapshot
                    //     (ticker task 会按 60 FPS 推送到前端)
                    evaluate_all_graphs_with(&eval2, f);
                    // 3.3 从 snapshot 收集派生值, push 到 buffer.derived_buffers
                    //     与本帧 push_frame 的时间戳对齐
                    {
                        let mut buf = buf2.lock();
                        push_derived_from_snapshot(&eval2, &mut buf);
                    }
                }
                frame_batch.extend(frames);
            }

            // 4. 批量 emit 帧 (一次 IPC 替代 N 次)
            if !frame_batch.is_empty() {
                let _ = app2.emit("transport:frames", &frame_batch);
                frame_batch.clear();
            }

            // 5. 统计节流 emit
            let now = Instant::now();
            if now.duration_since(last_stats).as_millis() >= STATS_THROTTLE_MS {
                let _ = app2.emit(
                    "transport:rx",
                    &TransportStats {
                        rx_bytes: acc_bytes,
                        rx_frames: acc_frames,
                        tx_bytes: 0,
                        tx_frames: 0,
                    },
                );
                acc_bytes = 0;
                acc_frames = 0;
                last_stats = now;
            }
        }

        // mpsc 关闭 → 传输已断开
        let _ = app2.emit("transport:state", ConnectionState::Disconnected);
        log::info!("解析任务已退出");
    });

    // data_loop: 快速消费 broadcast, 转发到 mpsc (不阻塞在 emit/解析上)
    loop {
        match rx.recv().await {
            Ok(data) => {
                if parse_tx.send(data).await.is_err() {
                    log::info!("解析任务已退出, 停止数据循环");
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                log::info!("数据广播通道已关闭");
                break;
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                log::warn!("数据广播落后 {} 条", n);
            }
        }
    }

    // 关闭 mpsc, 等待解析 task 刷完剩余数据
    drop(parse_tx);
    let _ = parse_task.await;
    log::info!("数据循环已退出");
}

/// 图输出推送循环 — 60 FPS 推送 output_snapshot 到所有订阅者
///
/// 订阅者通过 invoke('subscribe_graph_outputs', on_event: Channel) 加入
/// Channel 关闭时自动移除
pub async fn graph_output_ticker(state: GraphEvalState) {
    log::info!("图输出 ticker 已启动 (60 FPS)");
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(16));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        let snap = state.output_snapshot.lock().clone();
        let mut subs = state.output_subscribers.lock();
        // 尝试推送, 失败 (Channel 关闭) 则移除
        subs.retain(|ch| ch.send(snap.clone()).is_ok());
    }
}

/// Custom 输入推送循环 — 30 FPS 推送 Custom 输入到所有订阅者
///
/// 订阅者通过 invoke('subscribe_custom_inputs', on_event: Channel) 加入
pub async fn custom_input_ticker(state: GraphEvalState) {
    log::info!("Custom 输入 ticker 已启动 (30 FPS)");
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(33));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        // 仅当存在 Custom 节点时才收集
        let has_custom = state
            .graphs
            .lock()
            .values()
            .any(|g| !g.custom_node_ids().is_empty());
        if !has_custom {
            continue;
        }
        // 收集 Custom 输入
        let snap = state.output_snapshot.lock();
        let graphs = state.graphs.lock();
        let mut inputs: HashMap<String, HashMap<String, f32>> = HashMap::new();
        for (_, graph) in graphs.iter() {
            let ci = graph.collect_custom_inputs(&snap.values);
            for (k, v) in ci {
                inputs.insert(k, v);
            }
        }
        drop(snap);
        drop(graphs);

        if inputs.is_empty() {
            continue;
        }
        let batch = CustomInputBatch { inputs };
        let mut subs = state.custom_input_subscribers.lock();
        subs.retain(|ch| ch.send(batch.clone()).is_ok());
    }
}

/// 同步 spectrum_analyzers 与 graphs 中的 SpectrumSink 节点
///
/// - 遍历所有 graph 的 spectrum_sink_ids, 对每个 sink:
///   - 若 analyzer 不存在 → 按当前 config 创建
///   - 若 analyzer 存在但 config 变了 (window_size/window_type/output/sample_rate) → 重建
/// - 删除 graphs 中已不存在的 sink 对应的 analyzer
/// - 同时清理 spectrum_snapshot 中已不存在的 sink
///
/// 由 spectrum_ticker 在每 tick 开头调用, 保证 analyzer 与图拓扑一致。
fn sync_spectrum_analyzers(state: &GraphEvalState) {
    let graphs = state.graphs.lock();
    let mut analyzers = state.spectrum_analyzers.lock();

    // 收集所有 graph 中当前的 SpectrumSink id → config
    let mut current_configs: HashMap<String, (usize, vofa_next_dsp::WindowType, vofa_next_dsp::SpectrumOutput, f32)> = HashMap::new();
    for (_, graph) in graphs.iter() {
        for sink_id in graph.spectrum_sink_ids() {
            if let Some(cfg) = graph.spectrum_sink_config(&sink_id) {
                current_configs.insert(sink_id, cfg);
            }
        }
    }

    // 删除已不存在的 sink 的 analyzer
    analyzers.retain(|id, _| current_configs.contains_key(id));
    {
        let mut snap = state.spectrum_snapshot.lock();
        snap.retain(|id, _| current_configs.contains_key(id));
    }

    // 新建或重建 analyzer
    for (sink_id, (window_size, window_type, output, sample_rate)) in &current_configs {
        let need_rebuild = match analyzers.get(sink_id) {
            None => true,
            Some(a) => {
                // 任一配置变化都需要重建 (window_size/sample_rate 需要 new FFT planner;
                // window_type/output 虽有 setter 但重建更简单且不影响性能)
                a.window_size() != *window_size
                    || a.sample_rate() != *sample_rate
                    || a.window_type() != *window_type
                    || a.output() != *output
            }
        };
        if need_rebuild {
            let analyzer = SpectrumAnalyzer::new(
                *window_size,
                *window_type,
                *output,
                *sample_rate,
            );
            analyzers.insert(sink_id.clone(), analyzer);
            log::info!(
                "频谱分析器已 (重新)创建: sink={} window={} output={} fs={}",
                sink_id,
                window_size,
                match output {
                    vofa_next_dsp::SpectrumOutput::Magnitude => "Magnitude",
                    vofa_next_dsp::SpectrumOutput::Power => "Power",
                    vofa_next_dsp::SpectrumOutput::PSD => "PSD",
                    vofa_next_dsp::SpectrumOutput::Decibel => "Decibel",
                },
                sample_rate
            );
        }
    }
}

/// 频谱分析推送循环 — 30 FPS 触发 FFT + 推送结果到所有订阅者
///
/// 订阅者通过 invoke('subscribe_spectrum', on_event: Channel) 加入
/// Channel 关闭时自动移除
///
/// 流程:
/// 1. 每 tick 开头调用 sync_spectrum_analyzers 与 graphs 同步
/// 2. 对每个 analyzer 调用 compute() (窗口未填满返回 None, 跳过)
/// 3. 将结果存入 spectrum_snapshot
/// 4. 推送 SpectrumBatch 到所有订阅者
pub async fn spectrum_ticker(state: GraphEvalState) {
    log::info!("频谱分析 ticker 已启动 (30 FPS)");
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(33));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        // 1. 同步 analyzers 与 graphs
        sync_spectrum_analyzers(&state);

        // 2. 对每个 analyzer 计算 FFT
        let mut analyzers = state.spectrum_analyzers.lock();
        if analyzers.is_empty() {
            continue;
        }
        let mut new_results: HashMap<String, SpectrumResult> = HashMap::new();
        for (sink_id, analyzer) in analyzers.iter_mut() {
            if let Some(result) = analyzer.compute() {
                new_results.insert(sink_id.clone(), result);
            }
        }
        drop(analyzers);

        if new_results.is_empty() {
            continue;
        }

        // 3. 更新 spectrum_snapshot
        {
            let mut snap = state.spectrum_snapshot.lock();
            for (k, v) in &new_results {
                snap.insert(k.clone(), v.clone());
            }
        }

        // 4. 推送到所有订阅者 (snapshot 全量推送, 保证新订阅者立即收到数据)
        let batch = SpectrumBatch {
            spectra: state.spectrum_snapshot.lock().clone(),
        };
        let mut subs = state.spectrum_subscribers.lock();
        subs.retain(|ch| ch.send(batch.clone()).is_ok());
    }
}

/// CAN 帧订阅推送循环 — 按 interval_ms 推送最近 max_frames 个 CAN 帧
///
/// 由 subscribe_can_frames 命令 spawn, 通过 oneshot 接收取消信号优雅退出。
/// 推送的是缓冲区快照 (get_recent), 与 data_loop 的实时 emit 互补:
/// - data_loop emit "transport:can-frames": 实时批次 (适合需要逐帧处理的场景)
/// - can_frames_loop via Channel: 周期性快照 (适合 UI 列表展示, 控制刷新频率)
pub async fn can_frames_loop(
    buffer: Arc<Mutex<CanBuffer>>,
    on_event: Channel<CanFrameBatch>,
    interval: Duration,
    max_frames: usize,
    cancel_rx: oneshot::Receiver<()>,
) {
    let mut ticker = tokio::time::interval(interval);
    let mut cancel_rx = cancel_rx;
    loop {
        tokio::select! {
            _ = &mut cancel_rx => break,
            _ = ticker.tick() => {
                let frames = {
                    let buf = buffer.lock();
                    buf.get_recent(max_frames)
                };
                if on_event.send(CanFrameBatch { frames }).is_err() {
                    break;
                }
            }
        }
    }
}

/// 原始数据订阅推送循环 — 按 interval_ms 推送一批原始字节
///
/// 由 subscribe_rawdata 命令 spawn, 通过 oneshot 接收取消信号优雅退出。
/// 从 RawDataCollector 中 drain 最多 max_bytes 的完整块, 通过 Channel 推送到前端。
pub async fn rawdata_loop(
    collector: Arc<Mutex<RawDataCollector>>,
    on_event: Channel<vofa_next_buffer::RawDataBatch>,
    interval: Duration,
    max_bytes: usize,
    cancel_rx: oneshot::Receiver<()>,
) {
    let mut ticker = tokio::time::interval(interval);
    let mut cancel_rx = cancel_rx;
    loop {
        tokio::select! {
            _ = &mut cancel_rx => break,
            _ = ticker.tick() => {
                let batch = {
                    let mut col = collector.lock();
                    col.drain_batch(max_bytes)
                };
                if batch.chunks.is_empty() {
                    continue;
                }
                if on_event.send(batch).is_err() {
                    break;
                }
            }
        }
    }
}

fn now_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}
