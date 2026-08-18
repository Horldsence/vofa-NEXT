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
pub mod reconcile;

use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use vofa_next_buffer::{DataBuffer, RawDataCollector, RawDataDirection};
use vofa_next_core::{
    CanBuffer, CanLoadStats, ConnectionState, DecodedBuffer, LogicBuffer, PipelineConfig,
    ProtocolConfig, TransportStats,
};
use vofa_next_nodes::{BytePlan, NodeDef, NodeKind, SourceFramesMap};
use vofa_next_protocol::ProtocolEngine;
use vofa_next_transport::TransportManager;

use crate::pipeline::feed_parallel::ParallelFeeder;
use crate::state::GraphEvalState;

const STATS_THROTTLE_MS: u128 = 100;
/// 诊断指标输出间隔
const METRICS_REPORT_INTERVAL: Duration = Duration::from_secs(2);

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
    pub fn new(config: &ProtocolConfig, convert_to: Option<&ProtocolConfig>) -> Self {
        Self {
            engine: Arc::new(Mutex::new(vofa_next_protocol::create_engine(config))),
            convert_engine: convert_to
                .map(|c| Arc::new(Mutex::new(vofa_next_protocol::create_engine(c)))),
            config: config.clone(),
            convert_config: convert_to.cloned(),
            parallel: Arc::new(tokio::sync::Mutex::new(ParallelFeeder::new())),
            in_parallel: false,
            parallel_supported: None,
            detection_notified: false,
        }
    }

    /// 图配置与运行时配置是否一致 (ProtocolConfig 无 PartialEq, 用 serde 值比较)
    fn matches(&self, config: &ProtocolConfig, convert_to: Option<&ProtocolConfig>) -> bool {
        serde_json::to_value(&self.config).ok() == serde_json::to_value(config).ok()
            && serde_json::to_value(&self.convert_config).ok()
                == serde_json::to_value(&convert_to).ok()
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
    /// 新增/配置变更 → 重建引擎; 节点删除 → 移除状态并清理 source_frames 对应项
    pub fn sync_protocol_states(&self) {
        let nodes = self.global_nodes.lock();
        let mut states = self.protocol_states.lock();
        // 移除已不存在的 Protocol 节点
        states.retain(|id, _| {
            matches!(nodes.get(id).map(|n| &n.kind), Some(NodeKind::Protocol { .. }))
        });
        // 新增 / 配置变更重建
        for n in nodes.values() {
            if let NodeKind::Protocol { config, convert_to } = &n.kind {
                match states.get(&n.id) {
                    Some(st) => {
                        let mut st = st.lock();
                        if !st.matches(config, convert_to.as_ref()) {
                            *st = ProtocolNodeState::new(config, convert_to.as_ref());
                        }
                    }
                    None => {
                        states.insert(
                            n.id.clone(),
                            Arc::new(Mutex::new(ProtocolNodeState::new(
                                config,
                                convert_to.as_ref(),
                            ))),
                        );
                    }
                }
            }
        }
        drop(states);
        // source_frames 清理由 protocol_states 存活集决定
        let live: Vec<String> = self.protocol_states.lock().keys().cloned().collect();
        self.source_frames
            .lock()
            .retain(|id, _| live.iter().any(|k| k == id));
    }

    /// 挂载 Transport 节点读任务 (open 成功后调用; 同 id 重复调用先 detach)
    pub async fn attach(&self, app: AppHandle, node_id: &str) {
        self.detach(node_id);
        let rx = self.transport.lock().await.subscribe(node_id);
        let Some(rx) = rx else {
            log::warn!("读任务挂载失败: 传输节点未打开: {}", node_id);
            return;
        };
        // 确保按源 raw 收集器存在 (rx 方向)
        self.raw_collector_for(node_id);
        let plane = self.clone();
        let id = node_id.to_string();
        let handle = tokio::spawn(read_task(app, plane, id.clone(), rx));
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
struct DataPlaneMetrics {
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
            log::warn!("{}", msg);
        } else {
            log::debug!("{}", msg);
        }
    }
}

/// Transport 节点读任务 — 每个 open 的 Transport 节点一个
///
/// 循环: subscribe 收字节 → 合批 (try_recv 排空, 上限取配置快照) → raw 收集 →
/// 沿全局 BytePlan 路由 (协议解析/帧解码喂入/回注发送) → 统计节流 emit。
/// 广播通道关闭 (连接断开) 时退出并 emit Disconnected。
async fn read_task(
    app: AppHandle,
    plane: DataPlaneState,
    node_id: String,
    mut rx: broadcast::Receiver<Vec<u8>>,
) {
    use tokio::sync::broadcast::error::{RecvError, TryRecvError};
    log::debug!("数据读任务已启动: {}", node_id);
    let mut dec_cache = crate::pipeline::decoder_feed::DecoderFeedCache::new();
    let mut last_stats = Instant::now();
    let mut acc_bytes: u64 = 0;
    let mut acc_frames: u64 = 0;
    let mut last_report = Instant::now();

    loop {
        let first = match rx.recv().await {
            Ok(d) => d,
            Err(RecvError::Closed) => break,
            Err(RecvError::Lagged(n)) => {
                plane.metrics.lagged.fetch_add(n, Ordering::Relaxed);
                continue;
            }
        };

        // 自适应合批: try_recv 排空积压并拼接 (协议按字节流解析, 拼接语义安全;
        // 负载越高单批越大, 天然背压自适应)
        let cfg = *plane.pipeline_config.read();
        let mut data = first;
        let mut coalesced = 1usize;
        while coalesced < cfg.coalesce_max_msgs && data.len() < cfg.coalesce_max_bytes_kb * 1024 {
            match rx.try_recv() {
                Ok(mut next) => {
                    data.append(&mut next);
                    coalesced += 1;
                }
                Err(TryRecvError::Lagged(n)) => {
                    plane.metrics.lagged.fetch_add(n, Ordering::Relaxed);
                }
                Err(_) => break,
            }
        }
        plane.metrics.rx_msgs.fetch_add(1, Ordering::Relaxed);
        plane
            .metrics
            .rx_bytes
            .fetch_add(data.len() as u64, Ordering::Relaxed);

        // 按源原始字节收集 (不随解析积压丢失 — 收集在路由之前完成)
        plane
            .raw_collector_for(&node_id)
            .lock()
            .push_chunk(vofa_next_core::now_us(), RawDataDirection::Rx, &data);

        // 沿全局 BytePlan 路由 (深度提示取广播积压, 供并行解析判定)
        let t_feed = Instant::now();
        let summary =
            byte_router::route_bytes(&plane, Some(&app), &node_id, &data, rx.len(), &mut dec_cache)
                .await;
        plane
            .metrics
            .feed_ns
            .fetch_add(t_feed.elapsed().as_nanos() as u64, Ordering::Relaxed);
        plane.metrics.feed_batches.fetch_add(1, Ordering::Relaxed);
        plane
            .metrics
            .eval_ns
            .fetch_add(summary.eval_ns, Ordering::Relaxed);
        plane
            .metrics
            .frames_evaled
            .fetch_add(summary.frames, Ordering::Relaxed);

        // FrameDecoder 被喂入 → 快照评估一次 (decoder 输出来自 last_frame 缓存)
        if summary.decoders_fed {
            frame_dispatch::refresh_snapshot(&plane);
        }

        // 统计 (record_rx 由消费侧上报)
        plane
            .transport
            .lock()
            .await
            .record_rx(&node_id, data.len(), summary.frames);
        acc_bytes += data.len() as u64;
        acc_frames += summary.frames;

        // 统计节流 emit (100ms 窗口)
        if last_stats.elapsed().as_millis() >= STATS_THROTTLE_MS {
            crate::events::emit_transport_rx(
                &app,
                &node_id,
                TransportStats {
                    rx_bytes: acc_bytes,
                    rx_frames: acc_frames,
                    tx_bytes: 0,
                    tx_frames: 0,
                    rx_dropped: 0,
                },
            );
            acc_bytes = 0;
            acc_frames = 0;
            last_stats = Instant::now();
        }

        // 2s 诊断指标
        if last_report.elapsed() >= METRICS_REPORT_INTERVAL {
            plane.metrics.report();
            last_report = Instant::now();
        }
    }

    crate::events::emit_transport_state(&app, &node_id, ConnectionState::Disconnected);
    log::debug!("数据读任务已退出: {}", node_id);
}
