//! 应用全局状态 — AppState (Tauri-managed) + GraphEvalState 由
//! [`pipeline_data_plane`] 提供 (本文件 re-use 即可)。

use parking_lot::{Mutex, RwLock};
use pipeline_data_plane::{
    build_graph_eval_state, GraphEvalState, StreamGroupState, DEFAULT_CAN_BUFFER_CAPACITY,
    DEFAULT_CAN_LOAD_STATS_WINDOW, DEFAULT_DECODED_BUFFER_CAPACITY, DEFAULT_LOGIC_BUFFER_CAPACITY,
};
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tauri::ipc::Channel;
use tokio::sync::oneshot;
use buffer_raw::RawDataCollector;
use can_types::{CanBuffer, CanLoadStats};
use logic_types::{DecodedBuffer, LogicBuffer};
use vofa_core::PipelineConfig;
use node_engine::{CompiledGraph, SourceFramesMap};
use transport_core::TransportManager;

/// 应用全局状态
pub struct AppState {
    /// 传输注册表 (node_id → 连接实例; open 是异步的, 用 tokio mutex)
    pub transport: Arc<tokio::sync::Mutex<TransportManager>>,
    /// 数据平面状态 (字节平面 BytePlan / protocol_states / source_frames /
    /// 按源 buffers 与 raw_collectors / 读任务句柄) — 取代旧 protocol/protocol_config/
    /// buffer/raw_data_collector 单例字段
    pub data_plane: pipeline_data_plane::DataPlaneState,
    /// 节点图 — 按 tab_id 索引 (每个 tab 独立编译图)
    pub graphs: Arc<Mutex<HashMap<String, CompiledGraph>>>,
    /// 图版本号 (见 GraphEvalState::graphs_version)
    pub graphs_version: Arc<AtomicU64>,
    /// 输入控件当前值 (Knob/Slider/Button/Radio/Checkbox)
    /// key: widget_id, value: 当前值
    /// 由前端 invoke('set_input_value') 更新
    pub input_values: Arc<Mutex<HashMap<String, f32>>>,
    /// Custom widget 回传输出
    /// key: widget_id, value: portId -> value
    /// 由前端 invoke('submit_custom_output') 更新
    pub custom_outputs: Arc<Mutex<HashMap<String, HashMap<String, f32>>>>,
    /// 字符串输出 (Trigger 控件匹配字符串类型规则时写入)
    /// key: widget_id, value: portId -> string
    /// 由前端 invoke('submit_custom_text_output') 更新
    pub custom_text_outputs: Arc<Mutex<HashMap<String, HashMap<String, String>>>>,
    /// 字符串输出快照 (TextOutputSnapshot, 供 ticker 推送)
    pub text_output_snapshot: Arc<Mutex<pipeline_data_plane::StringOutputSnapshot>>,
    /// 字符串输出订阅者
    pub text_output_subscribers:
        Arc<Mutex<Vec<Channel<pipeline_data_plane::StringOutputSnapshot>>>>,
    /// 图输出订阅者 (60 FPS 推送)
    pub output_subscribers: Arc<Mutex<Vec<Channel<pipeline_data_plane::GraphOutputSnapshot>>>>,
    /// Custom 输入订阅者 (30 FPS 推送到前端 iframe)
    pub custom_input_subscribers:
        Arc<Mutex<Vec<Channel<pipeline_data_plane::CustomInputBatch>>>>,
    /// FrameDecoder 节点旁路原始字节收集器 (供前端 RawData 显示"每帧消费的原始字节")
    /// key: FrameDecoder widget id, value: Arc<Mutex<RawDataCollector>> (共享实例)
    /// 与 decoder_states 生命周期同步, 独立于按 Transport 源的 raw_collectors
    pub decoder_raw_collectors: Arc<Mutex<HashMap<String, Arc<Mutex<RawDataCollector>>>>>,
    /// 频谱订阅者 (30 FPS 推送)
    pub spectrum_subscribers: Arc<Mutex<Vec<Channel<pipeline_data_plane::SpectrumBatch>>>>,
    /// 波形订阅任务的取消句柄 — key: channel_id, value: oneshot sender
    /// 前端调用 unsubscribe_waveform 时, 通过 channel_id 取出 sender 发送取消信号,
    /// 让 tokio::spawn 的 task 优雅退出, 避免向已关闭的 channel send 产生警告。
    pub waveform_tasks: Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>,
    /// 原始数据订阅任务的取消句柄
    pub raw_data_tasks: Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>,
    /// 流订阅组注册表 — key: 组 id (首个分片的 channel_id 字符串)
    /// 统一分片框架 (pipeline::stream): RAWDATA/波形/CAN/逻辑/解码共用;
    /// 分片任务退出时 shards-1, 空组移除
    pub stream_groups: Arc<Mutex<HashMap<String, StreamGroupState>>>,
    /// FrameDecoder 节点原始数据订阅任务的取消句柄 — key: channel_id
    /// 前端调用 unsubscribe_rawdata_node 时, 通过 channel_id 取出 sender 发送取消信号
    pub raw_data_node_tasks: Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>,
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
    /// 流水线参数 (合批/并行解析/流分片/通道容量) — 由 set_pipeline_config 更新,
    /// 数据平面读任务 / 流订阅命令读取
    pub pipeline_config: Arc<RwLock<PipelineConfig>>,
}

impl AppState {
    pub fn new() -> Self {
        let transport = Arc::new(tokio::sync::Mutex::new(TransportManager::new()));
        let graphs = Arc::new(Mutex::new(HashMap::new()));
        let graphs_version = Arc::new(AtomicU64::new(0));
        let input_values = Arc::new(Mutex::new(HashMap::new()));
        let custom_outputs = Arc::new(Mutex::new(HashMap::new()));
        let custom_text_outputs = Arc::new(Mutex::new(HashMap::new()));
        let text_output_snapshot =
            Arc::new(Mutex::new(pipeline_data_plane::StringOutputSnapshot::default()));
        let text_output_subscribers = Arc::new(Mutex::new(Vec::new()));
        let source_frames = Arc::new(Mutex::new(SourceFramesMap::default()));
        let output_snapshot = Arc::new(Mutex::new(pipeline_data_plane::GraphOutputSnapshot {
            tick: 0,
            graphs_version: 0,
            values: node_engine::ValuesMap::default(),
        }));
        let output_subscribers = Arc::new(Mutex::new(Vec::new()));
        let custom_input_subscribers = Arc::new(Mutex::new(Vec::new()));
        let filter_states = Arc::new(Mutex::new(HashMap::new()));
        let decoder_states = Arc::new(Mutex::new(HashMap::new()));
        let decoder_raw_collectors = Arc::new(Mutex::new(HashMap::new()));
        let spectrum_analyzers = Arc::new(Mutex::new(HashMap::new()));
        let spectrum_snapshot = Arc::new(Mutex::new(HashMap::new()));
        let spectrum_subscribers = Arc::new(Mutex::new(Vec::new()));
        let ifft_states = Arc::new(Mutex::new(HashMap::new()));
        let can_buffer = Arc::new(Mutex::new(CanBuffer::new(DEFAULT_CAN_BUFFER_CAPACITY)));
        // DEFAULT_CAN_LOAD_STATS_WINDOW = (window_us, history_capacity)
        let (window_us, history_capacity) = DEFAULT_CAN_LOAD_STATS_WINDOW;
        let can_load_stats =
            Arc::new(Mutex::new(CanLoadStats::new(window_us, history_capacity)));
        let logic_buffer = Arc::new(Mutex::new(LogicBuffer::new(DEFAULT_LOGIC_BUFFER_CAPACITY)));
        let decoded_buffer =
            Arc::new(Mutex::new(DecodedBuffer::new(DEFAULT_DECODED_BUFFER_CAPACITY)));
        let pipeline_config = Arc::new(RwLock::new(PipelineConfig::default()));

        let eval: GraphEvalState = build_graph_eval_state(
            graphs.clone(),
            graphs_version.clone(),
            input_values.clone(),
            custom_outputs.clone(),
            text_output_snapshot.clone(),
            text_output_subscribers.clone(),
            custom_text_outputs.clone(),
            source_frames.clone(),
            output_snapshot.clone(),
            output_subscribers.clone(),
            custom_input_subscribers.clone(),
            filter_states,
            decoder_states,
            decoder_raw_collectors.clone(),
            spectrum_analyzers,
            spectrum_snapshot,
            spectrum_subscribers.clone(),
            ifft_states,
        );
        let data_plane = pipeline_data_plane::DataPlaneState::new(
            transport.clone(),
            eval,
            source_frames,
            can_buffer.clone(),
            can_load_stats.clone(),
            logic_buffer.clone(),
            decoded_buffer.clone(),
            pipeline_config.clone(),
        );

        Self {
            transport,
            data_plane,
            graphs,
            graphs_version,
            input_values,
            custom_outputs,
            custom_text_outputs,
            text_output_snapshot,
            text_output_subscribers,
            output_subscribers,
            custom_input_subscribers,
            decoder_raw_collectors,
            spectrum_subscribers,
            waveform_tasks: Arc::new(Mutex::new(HashMap::new())),
            raw_data_tasks: Arc::new(Mutex::new(HashMap::new())),
            stream_groups: Arc::new(Mutex::new(HashMap::new())),
            raw_data_node_tasks: Arc::new(Mutex::new(HashMap::new())),
            can_buffer,
            can_load_stats,
            can_load_tasks: Arc::new(Mutex::new(HashMap::new())),
            can_tasks: Arc::new(Mutex::new(HashMap::new())),
            logic_buffer,
            decoded_buffer,
            logic_tasks: Arc::new(Mutex::new(HashMap::new())),
            decoded_tasks: Arc::new(Mutex::new(HashMap::new())),
            pipeline_config,
        }
    }

    /// 抽取图评估所需的 Arc 字段 (供 ticker / 数据平面持有)
    pub fn eval_state(&self) -> GraphEvalState {
        self.data_plane.eval.clone()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
