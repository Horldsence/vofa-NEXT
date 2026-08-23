//! # data_plane — 数据平面执行器 (取代旧 data_loop)
//!
//! 架构 (两平面节点图重构):
//! - **字节平面** (全局, 事件驱动): 每个 open 的 Transport 节点一个读任务,
//!   subscribe → record_rx → 按源 raw 收集 → 沿全局 [`BytePlan`] 推送
//!   (见 [`byte_router`]): Protocol.in 解析 / FrameDecoder.in 喂入 / Transport.tx 发送;
//!   Protocol 节点的 convert_to 输出引擎把帧重编码为字节继续沿 `out` 边下推。
//! - **数值平面** (每 tab f32 槽位): Protocol 节点产帧 → [`frame_dispatch`]
//!   写 source_frames 缓存 + 触发引用该源的 tab 图评估 (见 [`crate::pipeline::graph_eval`])。
//!
//! 与旧 data_loop 的对应关系:
//! - 合批: 读任务内 broadcast try_recv 排空 (上限取 PipelineConfig 快照), 语义不变
//! - 并行解析: feed_parallel 保留, ParallelFeeder 改为按 Protocol 节点持有
//! - 背压: broadcast Lagged 计数 + 2s 诊断指标 ([`DataPlaneMetrics`])
//! - force_eval 空帧机制删除: FrameDecoder 刷新改为字节事件后的快照评估
//!   ([`frame_dispatch::refresh_snapshot`], 以 source_frames 现状评估)

pub mod byte_router;
pub mod frame_dispatch;
pub mod read_task;
pub mod reconcile;

use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::task::JoinHandle;
use buffer_databuffer::DataBuffer;
use buffer_raw::RawDataCollector;
use can_types::{CanBuffer, CanLoadStats};
use logic_types::{DecodedBuffer, LogicBuffer};
use schema_types::{ProtocolConfig, ProtocolSchema};
use vofa_core::PipelineConfig;
use node_engine::{BytePlan, SourceFramesMap, SourceTextsMap};
use node_kind::{NodeDef, NodeKind};
use protocol_engine::ProtocolEngine;
use transport_core::TransportManager;

use crate::eval_state::GraphEvalState;
use crate::feed_parallel::ParallelFeeder;

use logic_decoder::LogicDecoderEngine;
use protocol_can_bridge::{CandleEngine as Candle, RawDataEngine as RawData, SlcanEngine as Slcan};
use protocol_float::{FireWaterEngine as FireWater, JustFloatEngine as JustFloat};

/// 根据配置创建协议引擎
fn create_protocol_engine(config: &ProtocolConfig) -> Box<dyn ProtocolEngine> {
    match config {
        ProtocolConfig::JustFloat { channels } => Box::new(JustFloat::new(*channels)),
        ProtocolConfig::FireWater { channels } => Box::new(FireWater::new(*channels)),
        ProtocolConfig::RawData => Box::new(RawData::new()),
        ProtocolConfig::Slcan => Box::new(Slcan::new()),
        ProtocolConfig::CandleLight => Box::new(Candle::new()),
        ProtocolConfig::LogicDecode { decoder } => Box::new(LogicDecoderEngine::new(decoder.clone())),
        ProtocolConfig::Diagnostic { .. } => Box::new(RawData::new()),
    }
}

/// 统计节流窗口 (TransportStats 上报间隔) — 100ms
pub const STATS_THROTTLE_MS: u128 = 100;
/// 诊断指标输出间隔
pub const METRICS_REPORT_INTERVAL: Duration = Duration::from_secs(2);

/// Protocol 节点运行时状态 — 生命周期跟随全局节点表 (图重编译时增删)
pub struct ProtocolNodeState {
    /// 解析引擎 (feed 同步, 锁内无 await)
    pub engine: Arc<Mutex<Box<dyn ProtocolEngine>>>,
    /// convert_to 输出引擎 (encode_frame 重编码, 协议转换链)
    pub convert_engine: Option<Arc<Mutex<Box<dyn ProtocolEngine>>>>,
    /// 当前协议配置 (set_protocol 可运行时覆盖; 图重编译时与图配置比对, 不一致则重建)
    pub config: ProtocolConfig,
    /// convert_to 目标配置
    pub convert_config: Option<ProtocolConfig>,
    /// 帧 schema (协议引擎统一为 schema 模型; None = 旧前端, 引擎按 config 构造)
    pub schema: Option<ProtocolSchema>,
    /// 并行解析编排器 (feed 内含 spawn_blocking await, 用 tokio mutex 跨 await 持有)
    pub parallel: Arc<tokio::sync::Mutex<ParallelFeeder>>,
    /// 当前是否处于并行解析模式 (顺序↔并行切换时做 pending 交接)
    pub in_parallel: bool,
    /// 协议是否支持并行解析 (None = 未探测, 空数据 split_aligned 探测一次)
    pub parallel_supported: Option<bool>,
    /// 自动通道检测通知是否已发 (一次性)
    pub detection_notified: bool,
}

impl ProtocolNodeState {
    pub fn new(
        config: &ProtocolConfig,
        convert_to: Option<&ProtocolConfig>,
        schema: Option<&ProtocolSchema>,
    ) -> Self {
        // 有 schema 时由 compile_schema 构造引擎 (预设走 legacy 引擎, Custom 走 SchemaEngine);
        // 无 schema (旧前端) 保持原有 create_engine 路径
        let engine = match schema {
            Some(s) => schema_engine::compile_schema(s),
            None => create_protocol_engine(config),
        };
        Self {
            engine: Arc::new(Mutex::new(engine)),
            convert_engine: convert_to
                .map(|c| Arc::new(Mutex::new(create_protocol_engine(c)))),
            config: config.clone(),
            convert_config: convert_to.cloned(),
            schema: schema.cloned(),
            parallel: Arc::new(tokio::sync::Mutex::new(ParallelFeeder::new())),
            in_parallel: false,
            parallel_supported: None,
            detection_notified: false,
        }
    }

    /// 图配置与运行时配置是否一致 (ProtocolConfig 无 PartialEq, 用 serde 值比较)
    fn matches(
        &self,
        config: &ProtocolConfig,
        convert_to: Option<&ProtocolConfig>,
        schema: Option<&ProtocolSchema>,
    ) -> bool {
        serde_json::to_value(&self.config).ok() == serde_json::to_value(config).ok()
            && serde_json::to_value(&self.convert_config).ok()
                == serde_json::to_value(convert_to).ok()
            && serde_json::to_value(&self.schema).ok() == serde_json::to_value(schema).ok()
    }
}

/// 数据平面共享状态 (Arc 共享, 仿 GraphEvalState 模式)
///
/// 由 AppState::new 构建, 各字段为 Arc 克隆; 读任务/命令通过 clone 持有。
#[derive(Clone)]
pub struct DataPlaneState {
    /// 传输注册表 (node_id → 连接实例)
    pub transport: Arc<tokio::sync::Mutex<TransportManager>>,
    /// 全局节点表 (所有 tab 合并, 按 id 覆盖; 全局 BytePlan 重建的依据)
    pub global_nodes: Arc<Mutex<HashMap<String, NodeDef>>>,
    /// 全局字节平面 (所有 tab byte_edges 合并重算)
    pub byte_plan: Arc<Mutex<BytePlan>>,
    /// Protocol 节点运行时状态 (key = Protocol 节点 id)
    pub protocol_states: Arc<Mutex<HashMap<String, Arc<Mutex<ProtocolNodeState>>>>>,
    /// 每源最新帧缓存 (key = Protocol 节点 id, latest-value 融合)
    pub source_frames: Arc<Mutex<SourceFramesMap>>,
    /// 每源最新文本缓存 (key = Protocol 节点 id; RawData 协议原始字节 UTF-8 lossy 解码,
    /// latest-value 融合) — ProtocolSource 的 "str" 端口 (String 域) 数据源
    pub source_texts: Arc<Mutex<SourceTextsMap>>,
    /// 每源数据缓冲区 (key = Protocol 节点 id; 派生键随实例隔离)
    pub buffers: Arc<Mutex<HashMap<String, Arc<Mutex<DataBuffer>>>>>,
    /// 每 Transport 节点 rx 的原始字节收集器
    pub raw_collectors: Arc<Mutex<HashMap<String, Arc<Mutex<RawDataCollector>>>>>,
    /// Transport 读任务句柄表 (key = Transport 节点 id)
    read_tasks: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    /// 最近一次已对悬空 ProtocolSource 发出警告的 graphs_version (reconcile 去重)
    pub(crate) reconcile_warn_version: Arc<AtomicU64>,
    /// 数值平面状态 (图/滤波/解码器/快照等)
    pub eval: GraphEvalState,
    pub can_buffer: Arc<Mutex<CanBuffer>>,
    pub can_load_stats: Arc<Mutex<CanLoadStats>>,
    pub logic_buffer: Arc<Mutex<LogicBuffer>>,
    pub decoded_buffer: Arc<Mutex<DecodedBuffer>>,
    pub pipeline_config: Arc<RwLock<PipelineConfig>>,
    /// 流水线诊断指标 (2s 窗口)
    metrics: Arc<DataPlaneMetrics>,
}

impl DataPlaneState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        transport: Arc<tokio::sync::Mutex<TransportManager>>,
        eval: GraphEvalState,
        source_frames: Arc<Mutex<SourceFramesMap>>,
        source_texts: Arc<Mutex<SourceTextsMap>>,
        can_buffer: Arc<Mutex<CanBuffer>>,
        can_load_stats: Arc<Mutex<CanLoadStats>>,
        logic_buffer: Arc<Mutex<LogicBuffer>>,
        decoded_buffer: Arc<Mutex<DecodedBuffer>>,
        pipeline_config: Arc<RwLock<PipelineConfig>>,
    ) -> Self {
        Self {
            transport,
            global_nodes: Arc::new(Mutex::new(HashMap::new())),
            byte_plan: Arc::new(Mutex::new(BytePlan::default())),
            protocol_states: Arc::new(Mutex::new(HashMap::new())),
            source_frames,
            source_texts,
            buffers: Arc::new(Mutex::new(HashMap::new())),
            raw_collectors: Arc::new(Mutex::new(HashMap::new())),
            read_tasks: Arc::new(Mutex::new(HashMap::new())),
            reconcile_warn_version: Arc::new(AtomicU64::new(u64::MAX)),
            eval,
            can_buffer,
            can_load_stats,
            logic_buffer,
            decoded_buffer,
            pipeline_config,
            metrics: Arc::new(DataPlaneMetrics::default()),
        }
    }

    /// 取指定源的数据缓冲区 (不存在则按默认容量创建: 100k 点 × 4 通道)
    pub fn buffer_for(&self, source: &str) -> Arc<Mutex<DataBuffer>> {
        self.buffers
            .lock()
            .entry(source.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(DataBuffer::new(100_000, 4))))
            .clone()
    }

    /// 取指定 Transport 节点的原始字节收集器 (不存在则创建)
    pub fn raw_collector_for(&self, source: &str) -> Arc<Mutex<RawDataCollector>> {
        self.raw_collectors
            .lock()
            .entry(source.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(RawDataCollector::new())))
            .clone()
    }

    /// 同步 protocol_states 与全局节点表中的 Protocol 节点 (图重编译后调用):
    /// 新增/配置变更 → 重建引擎; 节点删除 → 移除状态并清理 source_frames/source_texts 对应项
    pub fn sync_protocol_states(&self) {
        let nodes = self.global_nodes.lock();
        let mut states = self.protocol_states.lock();
        // 移除已不存在的 Protocol 节点
        states.retain(|id, _| {
            matches!(
                nodes.get(id).map(|n| &n.kind),
                Some(NodeKind::Protocol { .. })
            )
        });
        // 新增 / 配置变更重建
        for n in nodes.values() {
            if let NodeKind::Protocol {
                config,
                convert_to,
                schema,
            } = &n.kind
            {
                match states.get(&n.id) {
                    Some(st) => {
                        let mut st = st.lock();
                        if !st.matches(config, convert_to.as_ref(), schema.as_ref()) {
                            *st = ProtocolNodeState::new(config, convert_to.as_ref(), schema.as_ref());
                        }
                    }
                    None => {
                        states.insert(
                            n.id.clone(),
                            Arc::new(Mutex::new(ProtocolNodeState::new(
                                config,
                                convert_to.as_ref(),
                                schema.as_ref(),
                            ))),
                        );
                    }
                }
            }
        }
        drop(states);
        // source_frames / source_texts 清理由 protocol_states 存活集决定
        let live: Vec<String> = self.protocol_states.lock().keys().cloned().collect();
        self.source_frames
            .lock()
            .retain(|id, _| live.iter().any(|k| k == id));
        self.source_texts
            .lock()
            .retain(|id, _| live.iter().any(|k| k == id));
    }

    /// 挂载 Transport 节点读任务 (open 成功后调用; 同 id 重复调用先 detach)
    pub async fn attach(&self, app: AppHandle, node_id: &str) {
        self.detach(node_id);
        let rx = self.transport.lock().await.subscribe(node_id);
        let Some(rx) = rx else {
            log::warn!("读任务挂载失败: 传输节点未打开: {node_id}");
            return;
        };
        // 确保按源 raw 收集器存在 (rx 方向)
        self.raw_collector_for(node_id);
        let plane = self.clone();
        let id = node_id.to_string();
        let handle = tokio::spawn(read_task::read_task(app, plane, id.clone(), rx));
        self.read_tasks.lock().insert(id, handle);
    }

    /// 卸载 Transport 节点读任务 (close 时调用)
    pub fn detach(&self, node_id: &str) {
        if let Some(h) = self.read_tasks.lock().remove(node_id) {
            h.abort();
        }
    }
}

/// 流水线诊断指标 — 各 Transport 读任务共享, 每 2s 输出一次 (有活动时)。
#[derive(Default)]
pub struct DataPlaneMetrics {
    /// 收到的消息数 / 字节数 (合批后)
    rx_msgs: AtomicU64,
    rx_bytes: AtomicU64,
    /// broadcast Lagged 丢弃的消息数
    lagged: AtomicU64,
    /// 字节路由 + 协议解析累计耗时 ns / 批次数
    feed_ns: AtomicU64,
    feed_batches: AtomicU64,
    /// 数值平面评估累计耗时 ns / 帧数
    eval_ns: AtomicU64,
    frames_evaled: AtomicU64,
}

impl DataPlaneMetrics {
    fn report(&self) {
        let rx_msgs = self.rx_msgs.swap(0, Ordering::Relaxed);
        let lagged = self.lagged.swap(0, Ordering::Relaxed);
        if rx_msgs == 0 && lagged == 0 {
            return;
        }
        let secs = METRICS_REPORT_INTERVAL.as_secs_f64();
        let batches = self.feed_batches.swap(0, Ordering::Relaxed);
        let msg = format!(
            "数据平面指标: rx {:.1}MB/s ({} 消息/s) | feed {} 批, 均 {:.2}ms | \
             eval 均 {:.2}ms/批, 帧均 {}/批 | Lagged 丢弃 {} 条",
            self.rx_bytes.swap(0, Ordering::Relaxed) as f64 / secs / 1e6,
            (rx_msgs as f64 / secs) as u64,
            batches,
            self.feed_ns.swap(0, Ordering::Relaxed) as f64 / batches.max(1) as f64 / 1e6,
            self.eval_ns.swap(0, Ordering::Relaxed) as f64 / batches.max(1) as f64 / 1e6,
            self.frames_evaled.swap(0, Ordering::Relaxed) / batches.max(1),
            lagged,
        );
        if lagged > 0 {
            log::warn!("{msg}");
        } else {
            log::debug!("{msg}");
        }
    }
}
