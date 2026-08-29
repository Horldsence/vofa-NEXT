//! 应用全局状态 — AppState (Tauri-managed) + GraphEvalState 由
//! [`pipeline_data_plane`] 提供 (本文件 re-use 即可)。

use buffer_raw::RawDataCollector;
use can_types::{CanBuffer, CanLoadStats};
use logic_types::{DecodedBuffer, LogicBuffer};
use node_engine::{CompiledGraph, SourceFramesMap, SourceTextsMap};
use parking_lot::{Mutex, RwLock};
use pipeline_data_plane::{
    build_graph_eval_state, GraphEvalState, StreamGroupState, DEFAULT_CAN_BUFFER_CAPACITY,
    DEFAULT_CAN_LOAD_STATS_WINDOW, DEFAULT_DECODED_BUFFER_CAPACITY, DEFAULT_LOGIC_BUFFER_CAPACITY,
};
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use transport_core::TransportManager;
use vofa_core::PipelineConfig;

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
    /// 源图存储 — 连线拓扑的后端权威 (最近一次成功编译的 NodeDef/Edge/端口提示;
    /// 拓扑 op 与 `graph:source` 事件的数据源, 见 `cmd_graph::source_graph`)
    pub source_graphs: crate::SourceGraphs,
    /// 工作区存储 — widget 配置记录 / 画布位置 / tab 元数据的后端权威,
    /// 随图提交原子更新并落盘 `workspace.json` (见 `workspace` 模块)
    pub workspace: crate::WorkspaceState,
    /// 输入控件当前值 (Knob/Slider/Button/Radio/Checkbox)
    /// key: widget_id, value: 当前值
    /// 由前端 invoke('set_input_value') 更新
    pub input_values: Arc<Mutex<HashMap<String, f32>>>,
    /// Custom widget 回传输出
    /// key: widget_id, value: portId -> value
    /// 由前端 invoke('submit_custom_output') 更新
    pub custom_outputs: Arc<Mutex<HashMap<String, HashMap<String, f32>>>>,
    /// 字符串输出 (Custom JS widget 字符串输出回传通道;
    /// Trigger 的字符串规则输出已由后端图求值直接产出)
    /// key: widget_id, value: portId -> string
    /// 由前端 invoke('submit_custom_text_output') 更新
    pub custom_text_outputs: Arc<Mutex<HashMap<String, HashMap<String, String>>>>,
    /// 字符串输出快照 (TextOutputSnapshot, 供 ticker 推送)
    pub text_output_snapshot: Arc<Mutex<pipeline_data_plane::StringOutputSnapshot>>,
    /// FrameDecoder 节点旁路原始字节收集器 (供前端 RawData 显示"每帧消费的原始字节")
    /// key: FrameDecoder widget id, value: Arc<Mutex<RawDataCollector>> (共享实例)
    /// 与 decoder_states 生命周期同步, 独立于按 Transport 源的 raw_collectors
    pub decoder_raw_collectors: Arc<Mutex<HashMap<String, Arc<Mutex<RawDataCollector>>>>>,
    /// 所有显示数据流共享的订阅生命周期管理器。
    pub subscriptions: subscription::SubscriptionManager,
    /// 流订阅组注册表 — key: 组 id (首个分片的 channel_id 字符串)
    /// 统一分片框架 (pipeline::stream): RAWDATA/波形/CAN/逻辑/解码共用;
    /// 分片任务退出时 shards-1, 空组移除
    pub stream_groups: Arc<Mutex<HashMap<String, StreamGroupState>>>,
    /// CAN 帧缓冲区
    pub can_buffer: Arc<Mutex<CanBuffer>>,
    /// CAN 负载统计器 (滑动窗口)
    pub can_load_stats: Arc<Mutex<CanLoadStats>>,
    /// 逻辑采样缓冲区
    pub logic_buffer: Arc<Mutex<LogicBuffer>>,
    /// 解码事件缓冲区
    pub decoded_buffer: Arc<Mutex<DecodedBuffer>>,
    /// 流水线参数 (合批/并行解析/流分片/通道容量) — 由 set_pipeline_config 更新,
    /// 数据平面读任务 / 流订阅命令读取
    pub pipeline_config: Arc<RwLock<PipelineConfig>>,
}

impl AppState {
    pub fn new() -> Self {
        let transport = Arc::new(tokio::sync::Mutex::new(TransportManager::new()));
        let graphs = Arc::new(Mutex::new(HashMap::new()));
        let graphs_version = Arc::new(AtomicU64::new(0));
        let source_graphs: crate::SourceGraphs = Arc::new(Mutex::new(HashMap::new()));
        let workspace: crate::WorkspaceState =
            Arc::new(Mutex::new(crate::WorkspaceInner::default()));
        let input_values = Arc::new(Mutex::new(HashMap::new()));
        let custom_outputs = Arc::new(Mutex::new(HashMap::new()));
        let custom_text_outputs = Arc::new(Mutex::new(HashMap::new()));
        let text_output_snapshot = Arc::new(Mutex::new(
            pipeline_data_plane::StringOutputSnapshot::default(),
        ));
        let source_frames = Arc::new(Mutex::new(SourceFramesMap::default()));
        let source_texts = Arc::new(Mutex::new(SourceTextsMap::default()));
        let output_snapshot = Arc::new(Mutex::new(pipeline_data_plane::GraphOutputSnapshot {
            tick: 0,
            graphs_version: 0,
            values: node_engine::ValuesMap::default(),
        }));
        let filter_states = Arc::new(Mutex::new(HashMap::new()));
        let decoder_states = Arc::new(Mutex::new(HashMap::new()));
        let decoder_raw_collectors = Arc::new(Mutex::new(HashMap::new()));
        let spectrum_analyzers = Arc::new(Mutex::new(HashMap::new()));
        let spectrum_snapshot = Arc::new(Mutex::new(HashMap::new()));
        let ifft_states = Arc::new(Mutex::new(HashMap::new()));
        let can_buffer = Arc::new(Mutex::new(CanBuffer::new(DEFAULT_CAN_BUFFER_CAPACITY)));
        // DEFAULT_CAN_LOAD_STATS_WINDOW = (window_us, history_capacity)
        let (window_us, history_capacity) = DEFAULT_CAN_LOAD_STATS_WINDOW;
        let can_load_stats = Arc::new(Mutex::new(CanLoadStats::new(window_us, history_capacity)));
        let logic_buffer = Arc::new(Mutex::new(LogicBuffer::new(DEFAULT_LOGIC_BUFFER_CAPACITY)));
        let decoded_buffer = Arc::new(Mutex::new(DecodedBuffer::new(
            DEFAULT_DECODED_BUFFER_CAPACITY,
        )));
        let pipeline_config = Arc::new(RwLock::new(PipelineConfig::default()));

        let eval: GraphEvalState = build_graph_eval_state(
            graphs.clone(),
            graphs_version.clone(),
            input_values.clone(),
            custom_outputs.clone(),
            text_output_snapshot.clone(),
            custom_text_outputs.clone(),
            source_frames.clone(),
            source_texts.clone(),
            output_snapshot,
            filter_states,
            decoder_states,
            decoder_raw_collectors.clone(),
            spectrum_analyzers,
            spectrum_snapshot,
            ifft_states,
        );
        let data_plane = pipeline_data_plane::DataPlaneState::new(
            transport.clone(),
            eval,
            source_frames,
            source_texts,
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
            source_graphs,
            workspace,
            input_values,
            custom_outputs,
            custom_text_outputs,
            text_output_snapshot,
            decoder_raw_collectors,
            subscriptions: subscription::SubscriptionManager::new(),
            stream_groups: Arc::new(Mutex::new(HashMap::new())),
            can_buffer,
            can_load_stats,
            logic_buffer,
            decoded_buffer,
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
