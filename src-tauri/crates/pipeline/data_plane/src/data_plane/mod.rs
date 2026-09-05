//! # data_plane — 数据平面执行器 (取代旧 data_loop)
//!
//! 架构 (两平面节点图重构):
//! - **字节平面** (全局, 事件驱动): 每个 open 的 Transport 节点一个读任务,
//!   subscribe → record_rx → 按源 raw 收集 → 沿全局 [`BytePlan`] 推送
//!   (见 [`byte_router`]): Protocol.in 解析 / FrameDecoder.in 喂入 / Transport.tx 发送;
//!   Protocol 节点的 convert_to 输出引擎把帧重编码为字节继续沿 `out` 边下推。
//! - **数值平面** (每 tab f32 槽位): Protocol 节点产帧 → [`frame_dispatch`]
//!   写 source_frames 缓存 + 触发引用该源的 tab 图评估 (见 [`graph_eval`])。
//!
//! 与旧 data_loop 的对应关系:
//! - 合批: 读任务内 broadcast try_recv 排空 (上限取 PipelineConfig 快照), 语义不变
//! - 并行解析: feed_parallel 保留, ParallelFeeder 改为按 Protocol 节点持有
//! - 背压: broadcast Lagged 计数 + 2s 诊断指标 ([`DataPlaneMetrics`])
//! - force_eval 空帧机制删除: FrameDecoder 刷新改为字节事件后的快照评估
//!   ([`frame_dispatch::refresh_snapshot`], 以 source_frames 现状评估)

pub mod byte_router;
mod eval_queue;
pub mod eval_worker;
pub mod frame_dispatch;
mod metrics;
mod protocol_feed;
mod protocol_node_state;
pub mod read_task;
pub mod reconcile;
mod routes;

use buffer_databuffer::DataBuffer;
use buffer_raw::RawDataCollector;
use can_types::{CanBuffer, CanLoadStats};
use engine::{BytePlan, SourceFramesMap, SourceTextsMap};
use kind::NodeDef;
use logic_types::{DecodedBuffer, LogicBuffer};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::task::JoinHandle;
use transport_core::TransportManager;
use vofa_core::{DataFrame, PipelineConfig};

use crate::eval_state::GraphEvalState;

pub use metrics::{DataPlaneMetrics, EvalDiagnostics, METRICS_REPORT_INTERVAL};
pub use protocol_node_state::ProtocolNodeState;
pub use protocol_node_state::SampleClockBasis;

/// 统计节流窗口 (TransportStats 上报间隔) — 100ms
pub const STATS_THROTTLE_MS: u128 = 100;

/// 数据缓冲区默认通道数 (buffer_for 懒建 / 自动模式引擎重建后待重新检测时的回退值)
pub const DEFAULT_BUFFER_CHANNELS: usize = 4;

/// 评估队列 (字节平面 → 数值平面解耦点): 每源有界批队列, 帧 Arc 共享 (不变量 4)
use eval_queue::FrameQueue;
/// 字节路由去重组表: 字节源 → [(等价配置 key, 代表节点, 组员)]
pub(crate) type RouteGroups = HashMap<String, Vec<(String, String, Vec<String>)>>;

/// 容量整备的目标窗口秒数 — 覆盖默认 2s 视图 + overscan + 平移余量
pub(crate) const BUFFER_WINDOW_TARGET_SECONDS: f64 = 2.5;

/// 原始层容量的绝对上限 (防病态分配; 超出部分由金字塔层承担)
pub(crate) const MAX_RAW_POINTS: usize = 16_000_000;

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
    /// 每源待评估帧批队列 (字节平面 → 数值平面解耦点; key = Protocol 节点 id;
    /// 元素 Arc 共享 — 去重组 fan-out 零拷贝, 不变量 4)
    pub(crate) frame_queues: Arc<Mutex<HashMap<String, FrameQueue>>>,
    /// 每源最近一次容量整备的速率 (±5% 内不重复整备)
    tuned_rate: Arc<Mutex<HashMap<String, f64>>>,
    /// 字节路由去重组 (key = 字节源 id): (等价配置 key, 代表节点, 组员) —
    /// 同源同配置的 Protocol 节点只解析一次, fan-out 给各 tab (不变量 4)
    pub(crate) route_groups: Arc<Mutex<RouteGroups>>,
    /// 缓冲别名: 去重组组员 → 代表 (组员显示读代表缓冲, 原始数据只记一份)
    buffer_aliases: Arc<Mutex<HashMap<String, String>>>,
    /// 评估 worker 唤醒器 (push 后 notify_one; 许可语义保证不丢唤醒)
    pub(crate) eval_notify: Arc<tokio::sync::Notify>,
    /// 评估 worker 轮转游标 (公平轮询各源队列)
    eval_cursor: Arc<AtomicU64>,
    /// 评估 worker 是否已启动 (单实例)
    eval_worker_started: Arc<AtomicBool>,
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
            frame_queues: Arc::new(Mutex::new(HashMap::new())),
            tuned_rate: Arc::new(Mutex::new(HashMap::new())),
            route_groups: Arc::new(Mutex::new(HashMap::new())),
            buffer_aliases: Arc::new(Mutex::new(HashMap::new())),
            eval_notify: Arc::new(tokio::sync::Notify::new()),
            eval_cursor: Arc::new(AtomicU64::new(0)),
            eval_worker_started: Arc::new(AtomicBool::new(false)),
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

    /// 取指定源的数据缓冲区 (不存在则按默认容量创建: 100k 点 × 默认通道数)。
    /// 去重组组员解析到代表缓冲 (原始数据只记录一份, 不变量 4)。
    pub fn buffer_for(&self, source: &str) -> Arc<Mutex<DataBuffer>> {
        let resolved = self
            .buffer_aliases
            .lock()
            .get(source)
            .cloned()
            .unwrap_or_else(|| source.to_string());
        self.buffers
            .lock()
            .entry(resolved)
            .or_insert_with(|| {
                Arc::new(Mutex::new(DataBuffer::new(
                    100_000,
                    DEFAULT_BUFFER_CHANNELS,
                )))
            })
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

    /// 解析产帧入评估队列 (字节平面 → 数值平面解耦点)
    ///
    /// 有界队列: 满时丢最旧整批并计数 (持续过载下保最新, 丢弃显式可观测),
    /// 原始记录不依赖本队列；超预算大批同样显式计为缺口。
    pub(crate) fn enqueue_frames(&self, source_id: &str, frames: Arc<Vec<DataFrame>>) {
        // 惰性启动: loopback/mcp 等命令路径不经过 attach, 首次入队时保证 worker 存活
        self.ensure_eval_worker();
        {
            let mut queues = self.frame_queues.lock();
            let queue = queues.entry(source_id.to_string()).or_default();
            let dropped = queue.push(frames);
            if dropped > 0 {
                self.metrics.add_eval_dropped(dropped);
            }
        }
        // 队列锁已释放再唤醒, worker 与生产者不在锁上互等
        self.eval_notify.notify_one();
    }

    /// 公平轮询取一批待评估帧，并在同一队列锁下取得该批之前的缺口标记。
    pub(crate) fn pop_frame_batch(&self) -> Option<(String, Arc<Vec<DataFrame>>, bool)> {
        let mut queues = self.frame_queues.lock();
        let non_empty: Vec<String> = queues
            .iter()
            .filter(|(_, queue)| !queue.is_empty())
            .map(|(key, _)| key.clone())
            .collect();
        let key = match non_empty.len() {
            0 => return None,
            1 => non_empty.into_iter().next()?,
            n => {
                let n64 = u64::try_from(n).unwrap_or(u64::MAX);
                let cursor = self.eval_cursor.fetch_add(1, Ordering::Relaxed) % n64;
                let index = usize::try_from(cursor).unwrap_or(0);
                non_empty
                    .into_iter()
                    .nth(index)
                    .expect("non_empty 长度已校验")
            }
        };
        queues.get_mut(&key).and_then(|queue| {
            queue.pop_ready().map(|(frames, enqueued, gap)| {
                self.metrics.queue_wait_max_ns.fetch_max(
                    u64::try_from(enqueued.elapsed().as_nanos()).unwrap_or(u64::MAX),
                    Ordering::Relaxed,
                );
                (key, frames, gap)
            })
        })
    }

    /// 清空全部源的评估队列 (工作区暂停/停止边沿用) — 不积压暂停期间的已解析帧,
    /// 返回丢弃帧数。调用方持 `execution.boundary` 写锁时执行, 与在途评估批次互斥。
    pub fn clear_all_eval_queues(&self) -> u64 {
        let mut dropped = 0_u64;
        for queue in self.frame_queues.lock().values_mut() {
            dropped += queue.clear();
        }
        dropped
    }

    /// 缓冲降载汇总 — 各源 storage_overflow 总和较上次报告的增量 (不变量 5:
    /// 丢弃显式化; 金字塔层对被覆盖部分提供包络, 波形不缺失)
    pub(crate) fn report_buffer_overflow_delta(&self) {
        let total: u64 = self
            .buffers
            .lock()
            .values()
            .map(|b| b.lock().storage_overflow())
            .sum();
        let prev = self
            .metrics
            .last_overflow_reported
            .swap(total, Ordering::Relaxed);
        let delta = total.wrapping_sub(prev);
        if delta > 0 {
            log::warn!(
                "缓冲降载: 原始层滚动覆盖 {delta} 样本 (2s), 窗口超出部分由金字塔层包络显示"
            );
        }
    }

    /// 同步排空评估队列 — 集成测试在 route_bytes 后立即断言 buffer/快照前调用
    /// (运行时由 eval worker 异步消费; 记录平面已在路由时入库, 此处只评估)
    pub fn flush_eval(&self) {
        while let Some((source, frames, gap)) = self.pop_frame_batch() {
            if gap {
                crate::graph_eval::reset_source_transient_state(&self.eval, &source);
            }
            let buffer = self.buffer_for(&source);
            let options = frame_dispatch::EvalOptions::from_config(&self.pipeline_config.read());
            let eval_ns = frame_dispatch::eval_frames(
                &self.eval,
                &self.global_nodes,
                &buffer,
                &source,
                &frames,
                options,
            );
            self.metrics.eval_ns.fetch_add(eval_ns, Ordering::Relaxed);
            self.metrics.eval_batches.fetch_add(1, Ordering::Relaxed);
            self.metrics
                .eval_batches_total
                .fetch_add(1, Ordering::Relaxed);
            self.metrics.frames_evaled.fetch_add(
                u64::try_from(frames.len()).unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
            self.metrics
                .eval_completed_total
                .fetch_add(frames.len() as u64, Ordering::Relaxed);
        }
    }

    /// 非破坏性诊断快照：累计值不随日志窗口清零，队列值不含执行中的批次。
    pub fn eval_diagnostics(&self) -> EvalDiagnostics {
        let queues = self.frame_queues.lock();
        EvalDiagnostics {
            queued_batches: queues.values().map(FrameQueue::len).sum(),
            queued_frames: queues.values().map(|q| q.frames).sum(),
            queued_estimated_bytes: queues.values().map(|q| q.bytes).sum(),
            queue_wait_max_ns: self.metrics.queue_wait_max_ns.load(Ordering::Relaxed),
            dispatch_wait_max_ns: self.metrics.dispatch_wait_max_ns.load(Ordering::Relaxed),
            eval_service_max_ns: self.metrics.eval_service_max_ns.load(Ordering::Relaxed),
            completed_frames: self.metrics.eval_completed_total.load(Ordering::Relaxed),
            completed_batches: self.metrics.eval_batches_total.load(Ordering::Relaxed),
            dropped_frames: self.metrics.eval_dropped_total.load(Ordering::Relaxed),
        }
    }

    /// 启动评估 worker (单实例; 首次 attach 时调用, 需在 tokio 运行时内)
    fn ensure_eval_worker(&self) {
        if !self.eval_worker_started.swap(true, Ordering::AcqRel) {
            let plane = self.clone();
            tokio::spawn(eval_worker::eval_worker(plane));
        }
    }
}
