use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::ipc::Channel;
use tokio::sync::oneshot;
use vofa_next_buffer::{DataBuffer, RawDataCollector};
use vofa_next_core::{
    CanBuffer, CanLoadStats, DecodedBuffer, LogicBuffer, ProtocolConfig,
};
use vofa_next_dsp::{DigitalFilter, SpectrumAnalyzer, SpectrumResult};
use vofa_next_nodes::{CompiledGraph, FrameParser};
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
    /// FrameDecoder 节点状态 (跨帧持久化, 字节流解析状态机)
    /// key: FrameDecoder widget id, value: FrameParser (含 buf/state/last_frame)
    /// 由 data_loop 在每包数据上调用 feed_frame_decoders 同步并喂入字节
    pub decoder_states: Arc<Mutex<HashMap<String, FrameParser>>>,
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
    /// FrameDecoder 节点状态 (跨帧持久化)
    pub decoder_states: Arc<Mutex<HashMap<String, FrameParser>>>,
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
    /// CAN 负载统计器 (滑动窗口)
    pub can_load_stats: Arc<Mutex<CanLoadStats>>,
    /// CAN 负载统计订阅任务的取消句柄 — key: channel_id
    pub can_load_tasks: Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>,
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
            decoder_states: Arc::new(Mutex::new(HashMap::new())),
            spectrum_analyzers: Arc::new(Mutex::new(HashMap::new())),
            spectrum_snapshot: Arc::new(Mutex::new(HashMap::new())),
            spectrum_subscribers: Arc::new(Mutex::new(Vec::new())),
            waveform_tasks: Arc::new(Mutex::new(HashMap::new())),
            raw_data_collector: Arc::new(Mutex::new(RawDataCollector::new())),
            raw_data_tasks: Arc::new(Mutex::new(HashMap::new())),
            can_buffer: Arc::new(Mutex::new(CanBuffer::new(100_000))),
            can_load_stats: Arc::new(Mutex::new(CanLoadStats::new(1_000_000, 120))),
            can_load_tasks: Arc::new(Mutex::new(HashMap::new())),
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
            decoder_states: self.decoder_states.clone(),
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
