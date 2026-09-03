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
pub mod eval_worker;
pub mod frame_dispatch;
pub mod read_task;
pub mod reconcile;

use buffer_databuffer::DataBuffer;
use buffer_raw::RawDataCollector;
use can_types::{CanBuffer, CanLoadStats};
use engine::{BytePlan, SourceFramesMap, SourceTextsMap};
use kind::{NodeDef, NodeKind};
use logic_types::{DecodedBuffer, LogicBuffer};
use parking_lot::{Mutex, RwLock};
use protocol_engine::ProtocolEngine;
use schema_types::{ProtocolConfig, ProtocolSchema};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::task::JoinHandle;
use transport_core::TransportManager;
use vofa_core::{DataFrame, PipelineConfig};

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
        ProtocolConfig::LogicDecode { decoder } => {
            Box::new(LogicDecoderEngine::new(decoder.clone()))
        }
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
    /// 自动通道检测通知是否已发 (一次性, 系统通知)
    pub detection_notified: bool,
    /// 上次已推送前端的自动通道检测值 (变化即推 `protocol:channels-detected`; None = 尚未推送)
    pub last_detected_pushed: Option<usize>,
    /// 来源采样时钟。线缆协议按读取批次解析时，同批样本原本共享到达时间；
    /// 这里按来源提供的精确采样率或线速估算恢复逐样本时间戳。
    sample_clock: Option<SampleClock>,
}

/// 采样时钟域 (数据平面不变量 1: 每源单一权威时钟, 流内不切换不混叠)
///
/// 帧时间戳 = 逻辑时间, 由**字节平面**在解析后一次性定案, 数值平面与显示端
/// 不做任何时间戳加工。到达时间只允许进入 Arrival 域 (来源无时钟声明时),
/// 且一条流的生命周期内域不可变 — 杜绝"采样时钟段"与"到达摊开段"在同一
/// 缓冲里交错 (波形折叠/畸变的根源)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SampleClockBasis {
    /// TestData: 配置采样率, 帧域逐样本精确推进 (rate = 帧/s)
    ExactRate,
    /// 串口: 波特率名义线速, 字节域确定推进 (rate = 字节/s, 含起止/校验位)
    SerialNominal,
}

/// 每源逻辑时钟 — 首批锁定域, 之后与到达节奏彻底解耦
enum SampleClock {
    /// 来源声明时钟: TestData 采样率 / 串口波特率线速
    Source {
        basis: SampleClockBasis,
        /// 名义速率: ExactRate = 帧/s; SerialNominal = 字节/s
        rate: f64,
        /// 下一未消费位置的逻辑时间 (µs): 帧域 = 下一帧; 字节域 = 下一字节
        next_us: f64,
        /// 首批锚点尚未消费 (首批把批尾锚定到到达时刻)
        anchored: bool,
    },
    /// 无时钟声明 (网络等): 到达域, 批内按到达区间线性摊开
    Arrival { next_us: f64 },
}

impl ProtocolNodeState {
    pub fn new(
        config: &ProtocolConfig,
        convert_to: Option<&ProtocolConfig>,
        schema: Option<&ProtocolSchema>,
    ) -> Self {
        // 有 schema 时由 compile_schema 构造引擎 (预设走 legacy 引擎, Custom 走 SchemaEngine);
        // 无 schema (旧前端) 保持原有 create_engine 路径
        let engine = schema.map_or_else(
            || create_protocol_engine(config),
            schema_engine::compile_schema,
        );
        Self {
            engine: Arc::new(Mutex::new(engine)),
            convert_engine: convert_to.map(|c| Arc::new(Mutex::new(create_protocol_engine(c)))),
            config: config.clone(),
            convert_config: convert_to.cloned(),
            schema: schema.cloned(),
            parallel: Arc::new(tokio::sync::Mutex::new(ParallelFeeder::new())),
            in_parallel: false,
            parallel_supported: None,
            detection_notified: false,
            last_detected_pushed: None,
            sample_clock: None,
        }
    }

    /// 为一批帧定案逻辑时间戳 (字节平面时间权威, 不变量 1)。
    ///
    /// - `hint`: `Some((basis, rate, batch_bytes))` = 来源声明的采样时钟
    ///   (TestData = 配置采样率; 串口 = 波特率线速 + 本批字节数); `None` = 无时钟声明。
    /// - `arrival_us`: 本批到达时刻 (仅用于首批锚点与 Arrival 域)。
    ///
    /// 域规则: **首批锁定时钟域, 流内不切换**。
    /// - Source 域: 时间由名义速率确定性推进; 采样率热更新只换步长保持相位;
    ///   hint 中途缺失 (运行态查询短暂失败) 沿用已锁定速率外推, 绝不落入到达域。
    /// - Arrival 域: 批尾 = 到达时刻, 批内在 (上一批尾, 本批尾] 区间线性摊开;
    ///   即使后续出现时钟声明也保持到达域 (不与历史段混写)。
    ///
    /// 任何情况下不重锚到到达时刻、不允许时间倒退。
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub(crate) fn restamp_frames(
        &mut self,
        hint: Option<(SampleClockBasis, f64, usize)>,
        arrival_us: u64,
        frames: &mut [vofa_core::DataFrame],
    ) {
        let n = frames.len();
        if n == 0 {
            // 无帧批次: 已锁定的串口字节时钟照常推进 (被消费字节的线速时间不丢失);
            // 域未锁定时不做任何决策, 留给首个非空批次
            if let (
                Some(SampleClock::Source {
                    basis: SampleClockBasis::SerialNominal,
                    rate,
                    next_us,
                    anchored,
                }),
                Some((SampleClockBasis::SerialNominal, _, batch_bytes)),
            ) = (&mut self.sample_clock, hint)
            {
                if !*anchored && batch_bytes > 0 {
                    *next_us += f64::from(u32::try_from(batch_bytes).unwrap_or(u32::MAX))
                        * 1_000_000.0
                        / *rate;
                }
            }
            return;
        }
        if self.sample_clock.is_none() {
            // 首批锁定时钟域; 锚点 = 本批尾到达时刻 (LIVE 窗口语义)
            self.sample_clock = Some(match hint {
                Some((basis, rate, _)) => SampleClock::Source {
                    basis,
                    rate,
                    next_us: arrival_us as f64,
                    anchored: true,
                },
                None => SampleClock::Arrival {
                    next_us: arrival_us as f64,
                },
            });
        }
        let arrival = arrival_us as f64;
        match (&mut self.sample_clock, hint) {
            (
                Some(SampleClock::Source {
                    basis,
                    rate,
                    next_us,
                    anchored,
                }),
                hint_now,
            ) => {
                // hint 缺失: 沿用已锁定域与速率外推 (串口按 0 字节 = 不推进)
                let (basis_now, rate_now, batch_bytes) = hint_now.unwrap_or((*basis, *rate, 0));
                // 域变更 (同节点换传输类型, 罕见): 保持时间连续, 只换推进公式
                *basis = basis_now;
                let step_us = match basis_now {
                    SampleClockBasis::ExactRate => {
                        // 采样率热更新: 相位保持, 只替换步长
                        if (*rate - rate_now).abs() > f64::EPSILON {
                            let previous_step_us = 1_000_000.0 / *rate;
                            *next_us += 1_000_000.0 / rate_now - previous_step_us;
                        }
                        *rate = rate_now;
                        1_000_000.0 / *rate
                    }
                    SampleClockBasis::SerialNominal => {
                        *rate = rate_now;
                        // 字节域: 本批字节的线速时间均摊到批内各帧。串口线以恒定
                        // 波特率送字节, 批内每帧步长 = 每帧字节数 × 位时间; 批间由
                        // next_us 连续累积吸收帧长波动 — 确定性推进, 与到达抖动/
                        // 合批大小无关。
                        f64::from(u32::try_from(batch_bytes).unwrap_or(u32::MAX)) * 1_000_000.0
                            / (*rate * n.max(1) as f64)
                    }
                };
                // 首批: 回退本批起点使末帧恰为到达锚点, 之后时间纯逻辑推进
                let mut ts = *next_us;
                if *anchored {
                    *anchored = false;
                    let span = step_us * (n - 1).min(100_000_000) as f64;
                    ts -= span;
                }
                for frame in frames.iter_mut() {
                    frame.timestamp = ts.max(0.0).round() as u64;
                    ts += step_us;
                }
                *next_us = ts;
            }
            (Some(SampleClock::Arrival { next_us }), _) => {
                let end = arrival.max(*next_us);
                if end > *next_us && n > 1 {
                    let span = end - *next_us;
                    for (i, frame) in frames.iter_mut().enumerate() {
                        frame.timestamp =
                            (*next_us + span * (i + 1) as f64 / n as f64).round() as u64;
                    }
                } else {
                    for frame in frames.iter_mut() {
                        frame.timestamp = end.round() as u64;
                    }
                }
                *next_us = end;
            }
            (None, _) => unreachable!("上方已初始化 sample_clock"),
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

/// 数据缓冲区默认通道数 (buffer_for 懒建 / 自动模式引擎重建后待重新检测时的回退值)
pub const DEFAULT_BUFFER_CHANNELS: usize = 4;

/// 评估队列 (字节平面 → 数值平面解耦点): 每源有界批队列, 帧 Arc 共享 (不变量 4)
pub(crate) type FrameQueue = VecDeque<Arc<Vec<DataFrame>>>;
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
    /// 每源自上次成功求值以来被丢弃的帧数 (缺口记账, 不变量 5) —
    /// eval worker 取批时消费, 触发有状态算子复位 + 告警
    eval_gaps: Arc<Mutex<HashMap<String, u64>>>,
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
            eval_gaps: Arc::new(Mutex::new(HashMap::new())),
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
    /// 新帧持续流动, 波形尾部始终最新。返回是否入队 (供诊断)。
    pub(crate) fn enqueue_frames(&self, source_id: &str, frames: Arc<Vec<DataFrame>>) {
        // 惰性启动: loopback/mcp 等命令路径不经过 attach, 首次入队时保证 worker 存活
        self.ensure_eval_worker();
        let dropped = {
            let mut queues = self.frame_queues.lock();
            let queue = queues.entry(source_id.to_string()).or_default();
            let mut dropped = 0_u64;
            while queue.len() >= eval_worker::EVAL_QUEUE_DEPTH {
                if let Some(old) = queue.pop_front() {
                    dropped += old.len() as u64;
                }
            }
            queue.push_back(frames);
            dropped
        };
        if dropped > 0 {
            self.metrics.add_eval_dropped(dropped);
            // 缺口记账 (不变量 5): eval 侧见到缺口即复位有状态算子并告警
            *self
                .eval_gaps
                .lock()
                .entry(source_id.to_string())
                .or_insert(0) += dropped;
        }
        // 队列锁已释放再唤醒, worker 与生产者不在锁上互等
        self.eval_notify.notify_one();
    }

    /// 公平轮询取一批待评估帧 (worker 消费)
    pub(crate) fn pop_frame_batch(&self) -> Option<(String, Arc<Vec<DataFrame>>)> {
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
        queues
            .get_mut(&key)
            .and_then(|queue| queue.pop_front().map(|frames| (key, frames)))
    }

    /// 取某源累计的求值缺口帧数 (有则清零返回; 无返回 None)
    pub(crate) fn take_eval_gap(&self, source_id: &str) -> Option<u64> {
        self.eval_gaps.lock().remove(source_id).filter(|n| *n > 0)
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

    /// 容量自洽 (不变量 2): 按来源名义帧率整备缓冲容量
    ///
    /// L0 目标容量 = 帧率 × 目标窗口秒数, 受内存预算半额折算的点数封顶
    /// (另一半留给派生层/金字塔/停止快照)。±5% 内的速率波动不重复整备。
    /// 超出封顶的窗口由金字塔层提供包络 (示波器语义)。
    pub(crate) fn tune_buffer_capacity(&self, source_id: &str, frames_per_sec: f64) {
        if !frames_per_sec.is_finite() || frames_per_sec <= 0.0 {
            return;
        }
        {
            let tuned = self.tuned_rate.lock();
            if tuned
                .get(source_id)
                .is_some_and(|r| (r - frames_per_sec).abs() / *r < 0.05)
            {
                return;
            }
        }
        let budget_mb =
            f64::from(u32::try_from(self.pipeline_config.read().memory_budget_mb).unwrap_or(256));
        let channels = f64::from(
            u32::try_from(self.buffer_for(source_id).lock().channel_count()).unwrap_or(4),
        )
        .max(1.0);
        // 每样本 ≈ 8B 时间戳 + 4B×通道; 半预算给原始层
        let bytes_per_point = 4.0f64.mul_add(channels, 8.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cap_points = (budget_mb * 0.5 * 1_048_576.0 / bytes_per_point) as usize;
        let cap = cap_points.min(MAX_RAW_POINTS);
        let buffer = self.buffer_for(source_id);
        let mut b = buffer.lock();
        if b.ensure_capacity_for_rate(frames_per_sec, BUFFER_WINDOW_TARGET_SECONDS, cap) {
            log::info!(
                "波形缓冲容量整备: 源 {source_id} 帧率 {frames_per_sec:.0}/s → {} 点 \
                 (目标窗口 {BUFFER_WINDOW_TARGET_SECONDS}s, 封顶 {cap})",
                b.max_points()
            );
        }
        self.tuned_rate
            .lock()
            .insert(source_id.to_string(), frames_per_sec);
    }

    /// 同步排空评估队列 — 集成测试在 route_bytes 后立即断言 buffer/快照前调用
    /// (运行时由 eval worker 异步消费; 记录平面已在路由时入库, 此处只评估)
    pub fn flush_eval(&self) {
        while let Some((source, frames)) = self.pop_frame_batch() {
            if self.take_eval_gap(&source).is_some() {
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
            self.metrics.frames_evaled.fetch_add(
                u64::try_from(frames.len()).unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        }
    }

    /// 启动评估 worker (单实例; 首次 attach 时调用, 需在 tokio 运行时内)
    fn ensure_eval_worker(&self) {
        if !self.eval_worker_started.swap(true, Ordering::AcqRel) {
            let plane = self.clone();
            tokio::spawn(eval_worker::eval_worker(plane));
        }
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
        let mut rebuilt: Vec<(String, ProtocolConfig)> = Vec::new();
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
                            *st = ProtocolNodeState::new(
                                config,
                                convert_to.as_ref(),
                                schema.as_ref(),
                            );
                            rebuilt.push((n.id.clone(), config.clone()));
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
                        rebuilt.push((n.id.clone(), config.clone()));
                    }
                }
            }
        }
        drop(states);
        drop(nodes);
        // 引擎 (重) 建后对齐该源 buffer 通道数: 手动 = 配置值;
        // 自动 = 检测值随引擎重置失效, 回默认通道数待重新检测 (set_channels 会清空已有数据)
        for (id, cfg) in rebuilt {
            let effective = cfg.manual_channels().unwrap_or(DEFAULT_BUFFER_CHANNELS);
            self.buffer_for(&id).lock().set_channels(effective);
        }
        // source_frames / source_texts / 评估队列清理由 protocol_states 存活集决定
        let live: Vec<String> = self.protocol_states.lock().keys().cloned().collect();
        self.source_frames
            .lock()
            .retain(|id, _| live.iter().any(|k| k == id));
        self.source_texts
            .lock()
            .retain(|id, _| live.iter().any(|k| k == id));
        self.frame_queues
            .lock()
            .retain(|id, _| live.iter().any(|k| k == id));
        self.eval_gaps
            .lock()
            .retain(|id, _| live.iter().any(|k| k == id));
        // 路由去重组与缓冲别名 (不变量 4): 同 (字节源, 协议配置等价) 只解析一次
        self.rebuild_route_groups();
    }

    /// 依据 BytePlan + 协议节点配置等价性重建去重组与缓冲别名 (冷路径,
    /// 图重编译后调用)。等价 key = (config, convert_to, schema) 的 serde 值。
    fn rebuild_route_groups(&self) {
        let consumers: Vec<(String, Vec<String>)> = {
            let plan = self.byte_plan.lock();
            plan.consumers
                .iter()
                .map(|(source, routes)| {
                    (
                        source.clone(),
                        routes.iter().map(|r| r.target.clone()).collect(),
                    )
                })
                .collect()
        };
        let nodes = self.global_nodes.lock();
        let states = self.protocol_states.lock();
        let mut groups: RouteGroups = HashMap::new();
        let mut aliases: HashMap<String, String> = HashMap::new();
        for (source, targets) in consumers {
            let mut protos: Vec<String> = targets
                .into_iter()
                .filter(|t| {
                    matches!(
                        nodes.get(t).map(|n| &n.kind),
                        Some(NodeKind::Protocol { .. })
                    )
                })
                .collect();
            protos.sort();
            let mut local: Vec<(String, String, Vec<String>)> = Vec::new();
            for target in protos {
                let key = states.get(&target).and_then(|st| {
                    let s = st.lock();
                    serde_json::to_string(&(&s.config, &s.convert_config, &s.schema)).ok()
                });
                let Some(key) = key else { continue };
                match local.iter_mut().find(|(k, ..)| *k == key) {
                    Some((_, _, members)) => members.push(target.clone()),
                    None => local.push((key, target.clone(), vec![target.clone()])),
                }
            }
            for (_, repr, members) in &local {
                for member in members {
                    aliases.insert(member.clone(), repr.clone());
                }
            }
            groups.insert(source, local);
        }
        *self.route_groups.lock() = groups;
        *self.buffer_aliases.lock() = aliases;
    }

    /// 挂载 Transport 节点读任务 (open 成功后调用; 同 id 重复调用先 detach)
    pub async fn attach(&self, app: AppHandle, node_id: &str) {
        self.ensure_eval_worker();
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
        let handle = self.read_tasks.lock().remove(node_id);
        if let Some(h) = handle {
            h.abort();
        }
    }

    /// 在主动中止读任务前同步发布下游断开状态；abort 不会执行 read_task 的退出清理。
    pub fn mark_source_disconnected(&self, node_id: &str) {
        read_task::mark_downstream_disconnected(self, node_id);
    }
}

/// 流水线诊断指标 — 各 Transport 读任务共享, 每 2s 输出一次 (有活动时)。
#[derive(Default)]
pub struct DataPlaneMetrics {
    /// 收到的消息数 (按广播消息逐条计数) / 字节数 (合批后)
    rx_msgs: AtomicU64,
    rx_bytes: AtomicU64,
    /// broadcast Lagged 丢弃的消息数
    lagged: AtomicU64,
    /// 字节路由累计耗时 ns (含协议解析与数值平面) / 批次数
    feed_ns: AtomicU64,
    feed_batches: AtomicU64,
    /// 数值平面评估累计耗时 ns / 帧数 (parse 均 = (feed_ns - eval_ns) / 批)
    eval_ns: AtomicU64,
    frames_evaled: AtomicU64,
    /// 评估队列溢出丢弃的帧数 (摄入/评估解耦后的显式降级计数)
    eval_dropped: AtomicU64,
    /// 上次报告时各源缓冲 storage_overflow 总和 (增量输出)
    last_overflow_reported: AtomicU64,
}

impl DataPlaneMetrics {
    /// 评估队列溢出时累计丢弃帧数 (eval worker 调用)
    pub fn add_eval_dropped(&self, frames: u64) {
        self.eval_dropped.fetch_add(frames, Ordering::Relaxed);
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )] // 诊断日志近似换算 (MB/s / ms), 数值精度不影响行为
    fn report(&self) {
        let rx_msgs = self.rx_msgs.swap(0, Ordering::Relaxed);
        let lagged = self.lagged.swap(0, Ordering::Relaxed);
        let eval_dropped = self.eval_dropped.swap(0, Ordering::Relaxed);
        if rx_msgs == 0 && lagged == 0 && eval_dropped == 0 {
            return;
        }
        let secs = METRICS_REPORT_INTERVAL.as_secs_f64();
        let batches = self.feed_batches.swap(0, Ordering::Relaxed).max(1);
        let feed_ns = self.feed_ns.swap(0, Ordering::Relaxed);
        let eval_ns = self.eval_ns.swap(0, Ordering::Relaxed);
        let frames = self.frames_evaled.swap(0, Ordering::Relaxed);
        // feed 含解析与数值平面两段, 拆开报告便于定位瓶颈段;
        // 产帧估算 = 已求值 + 求值丢弃 (记录平面不受丢弃影响, 不变量 3)
        let parse_ns = feed_ns.saturating_sub(eval_ns);
        let produced_per_sec = (frames.saturating_add(eval_dropped)) as f64 / secs;
        let msg = format!(
            "数据平面指标: rx {:.1}MB/s ({} 消息/s) | feed {} 批, 均 {:.2}ms \
             (parse 均 {:.2}ms | eval 均 {:.2}ms), 帧均 {}/批, 产帧≈{:.0}/s \
             | Lagged 丢弃 {} 条, 评估队列丢弃 {} 帧",
            self.rx_bytes.swap(0, Ordering::Relaxed) as f64 / secs / 1e6,
            (rx_msgs as f64 / secs) as u64,
            batches,
            feed_ns as f64 / batches as f64 / 1e6,
            parse_ns as f64 / batches as f64 / 1e6,
            eval_ns as f64 / batches as f64 / 1e6,
            frames / batches,
            produced_per_sec,
            lagged,
            eval_dropped,
        );
        if lagged > 0 || eval_dropped > 0 {
            log::warn!("{msg}");
        } else {
            log::debug!("{msg}");
        }
    }
}
