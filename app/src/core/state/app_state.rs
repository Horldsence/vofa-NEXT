use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use vofa_next_buffer::{DataBuffer, RawDataCollector};
use vofa_next_core::{
    CanBuffer, CanLoadStats, ConnectionState, DecodedBuffer, LogicBuffer, ProtocolConfig,
    TransportStats,
};
use vofa_next_dsp::{DigitalFilter, SpectrumAnalyzer, SpectrumResult};
use vofa_next_nodes::{CompiledGraph, FrameParser};
use vofa_next_protocol::ProtocolEngine;
use vofa_next_transport::TransportManager;

/// 单个图输出快照 — UI 每帧直接读取
///
/// values: widgetId -> portId -> value
/// 包含 ChannelSource/Input/Math/Custom/Filter 节点的输出
/// UI 通过 edges 自行解析 Sink 节点的输入 (上游 widgetId + sourceHandle)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphOutputSnapshot {
    /// 自增计数器, UI 可用于去重/丢弃过期帧
    pub tick: u64,
    /// widgetId -> portId -> value
    pub values: HashMap<String, HashMap<String, f32>>,
}

/// 节点图评估所需的共享状态 (从 AppState 抽取, 供 data_loop 使用)
pub struct GraphEvalState {
    pub graphs: Arc<Mutex<HashMap<String, CompiledGraph>>>,
    pub input_values: Arc<Mutex<HashMap<String, f32>>>,
    pub custom_outputs: Arc<Mutex<HashMap<String, HashMap<String, f32>>>>,
    pub output_snapshot: Arc<Mutex<GraphOutputSnapshot>>,
    /// Filter 节点状态 (跨帧持久化, 逐点滤波)
    /// key: Filter widget id, value: DigitalFilter (含 FIR 延迟线 / IIR biquad 状态)
    pub filter_states: Arc<Mutex<HashMap<String, DigitalFilter>>>,
    /// FrameDecoder 节点状态 (跨帧持久化, 字节流解析状态机)
    /// key: FrameDecoder widget id, value: FrameParser (含 buf/state/last_frame)
    /// 由 data_loop 在每包数据上调用 feed_frame_decoders 同步并喂入字节
    pub decoder_states: Arc<Mutex<HashMap<String, FrameParser>>>,
    /// SpectrumSink 节点对应的频谱分析器
    /// key: SpectrumSink widget id, value: SpectrumAnalyzer (含滑动窗口)
    pub spectrum_analyzers: Arc<Mutex<HashMap<String, SpectrumAnalyzer>>>,
    /// 最新一次 FFT 结果
    /// key: SpectrumSink widget id, value: SpectrumResult
    pub spectrum_snapshot: Arc<Mutex<HashMap<String, SpectrumResult>>>,
}

/// 应用全局状态
pub struct AppState {
    /// 传输管理器 (async mutex, 因为 open/send 是异步的)
    pub transport: tokio::sync::Mutex<TransportManager>,
    /// 协议引擎 (sync mutex, feed/encode 是同步的)
    pub protocol: Arc<Mutex<Box<dyn ProtocolEngine>>>,
    /// 当前协议配置
    pub protocol_config: Mutex<ProtocolConfig>,
    /// 连接状态 — UI 每帧直接读取 (由 open/close/data_loop 退出路径写入)
    pub connection_state: Arc<Mutex<ConnectionState>>,
    /// 传输统计 (节流写入, ~100ms) — UI 每帧直接读取
    pub stats: Arc<Mutex<TransportStats>>,
    /// 多通道数据缓冲区
    pub buffer: Arc<Mutex<DataBuffer>>,
    /// 节点图 — 按 tab_id 索引 (每个 tab 独立编译图)
    pub graphs: Arc<Mutex<HashMap<String, CompiledGraph>>>,
    /// 输入控件当前值 (Knob/Slider/Button/Radio/Checkbox)
    /// key: widget_id, value: 当前值
    pub input_values: Arc<Mutex<HashMap<String, f32>>>,
    /// Custom widget 回传输出
    /// key: widget_id, value: portId -> value
    pub custom_outputs: Arc<Mutex<HashMap<String, HashMap<String, f32>>>>,
    /// 最新一帧的图输出快照 (UI 每帧直接读取)
    pub output_snapshot: Arc<Mutex<GraphOutputSnapshot>>,
    /// Filter 节点状态 (跨帧持久化)
    pub filter_states: Arc<Mutex<HashMap<String, DigitalFilter>>>,
    /// FrameDecoder 节点状态 (跨帧持久化)
    pub decoder_states: Arc<Mutex<HashMap<String, FrameParser>>>,
    /// SpectrumSink 节点对应的频谱分析器
    pub spectrum_analyzers: Arc<Mutex<HashMap<String, SpectrumAnalyzer>>>,
    /// 最新一次 FFT 结果快照
    pub spectrum_snapshot: Arc<Mutex<HashMap<String, SpectrumResult>>>,
    /// 原始数据收集器
    pub raw_data_collector: Arc<Mutex<RawDataCollector>>,
    /// CAN 帧缓冲区
    pub can_buffer: Arc<Mutex<CanBuffer>>,
    /// CAN 负载统计器 (滑动窗口)
    pub can_load_stats: Arc<Mutex<CanLoadStats>>,
    /// 逻辑采样缓冲区
    pub logic_buffer: Arc<Mutex<LogicBuffer>>,
    /// 解码事件缓冲区
    pub decoded_buffer: Arc<Mutex<DecodedBuffer>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            transport: tokio::sync::Mutex::new(TransportManager::new()),
            protocol: Arc::new(Mutex::new(vofa_next_protocol::create_engine(
                &ProtocolConfig::default(),
            ))),
            protocol_config: Mutex::new(ProtocolConfig::default()),
            connection_state: Arc::new(Mutex::new(ConnectionState::Disconnected)),
            stats: Arc::new(Mutex::new(TransportStats::default())),
            buffer: Arc::new(Mutex::new(DataBuffer::new(100_000, 4))),
            graphs: Arc::new(Mutex::new(HashMap::new())),
            input_values: Arc::new(Mutex::new(HashMap::new())),
            custom_outputs: Arc::new(Mutex::new(HashMap::new())),
            output_snapshot: Arc::new(Mutex::new(GraphOutputSnapshot {
                tick: 0,
                values: HashMap::new(),
            })),
            filter_states: Arc::new(Mutex::new(HashMap::new())),
            decoder_states: Arc::new(Mutex::new(HashMap::new())),
            spectrum_analyzers: Arc::new(Mutex::new(HashMap::new())),
            spectrum_snapshot: Arc::new(Mutex::new(HashMap::new())),
            raw_data_collector: Arc::new(Mutex::new(RawDataCollector::new())),
            can_buffer: Arc::new(Mutex::new(CanBuffer::new(50_000))),
            can_load_stats: Arc::new(Mutex::new(CanLoadStats::new(1_000_000, 120))),
            logic_buffer: Arc::new(Mutex::new(LogicBuffer::new(20_000))),
            decoded_buffer: Arc::new(Mutex::new(DecodedBuffer::new(10_000))),
        }
    }

    /// 抽取图评估所需的 Arc 字段 (供 data_loop 持有)
    pub fn eval_state(&self) -> GraphEvalState {
        GraphEvalState {
            graphs: self.graphs.clone(),
            input_values: self.input_values.clone(),
            custom_outputs: self.custom_outputs.clone(),
            output_snapshot: self.output_snapshot.clone(),
            filter_states: self.filter_states.clone(),
            decoder_states: self.decoder_states.clone(),
            spectrum_analyzers: self.spectrum_analyzers.clone(),
            spectrum_snapshot: self.spectrum_snapshot.clone(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
