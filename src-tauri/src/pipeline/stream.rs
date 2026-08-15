//! # stream — 统一分片流框架
//!
//! 大数据流 (全局 RAWDATA / RawData 节点旁路 / 波形) 与小数据流 (CAN 帧 /
//! 逻辑采样 / 解码事件) 共用同一套订阅协议与分发机制:
//!
//! - **分片组**: 首个 Channel 建组, 后续 Channel 凭组 id 加入 (最多 [`MAX_STREAM_SHARDS`]),
//!   组内共享一个流源实例与组级单调 seq (在源锁内 fetch_add, 与 drain 顺序严格一致)。
//! - **自动并发**: shard 0 常活; shard i 仅在积压 ≥ i × [`StreamSource::ACTIVATION_UNIT`]
//!   时激活, 积压消退自动休眠 — 单 channel 够用不浪费, 不够自动多通道并行。
//! - **自适应**: [`AdaptiveRate`] 速率 (有数据提速到 16ms, 空闲退避) +
//!   `clamp(backlog, min_batch, MAX_DRAIN)` 批量。
//! - **顺序**: 增量流前端按 seq 严格重组; 快照流 (波形) 按 "最新 seq 胜出"。

use crate::pipeline::dispatcher::{adaptive_channel_loop, AdaptiveRate};
use crate::state::StreamGroupState;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::ipc::Channel;
use tokio::sync::oneshot;
use vofa_next_buffer::{DataBuffer, RawDataBatch, RawDataCollector, WaveformWindow};
use vofa_next_core::{
    CanBuffer, CanFrameBatch, DecodedBuffer, DecodedEventBatch, Error, LogicBuffer,
    LogicSampleBatch, Result,
};

/// 每个订阅组的最大分片数 — 默认值, 实际由 PipelineConfig::max_stream_shards 提供
/// (常量保留为默认值文档来源, 与 PipelineConfig::default() 保持同步)
#[allow(dead_code)]
pub const MAX_STREAM_SHARDS: usize = 4;

/// 统一流源 — 每种数据流实现一次, 即获得分片并发 + 自适应推送能力
pub trait StreamSource: Send + 'static {
    /// 推送批次 (必须可 serde 序列化, 且带 seq 字段)
    type Batch: serde::Serialize + Send + 'static;
    /// 当前积压量 (字节/条/帧), 供分片激活判断
    fn backlog(&mut self) -> usize;
    /// drain 一批; 无数据返回 None
    fn drain(&mut self, max: usize) -> Option<Self::Batch>;
    /// 写 seq (在源锁内调用, 保证与 drain 顺序一致)
    fn set_seq(batch: &mut Self::Batch, seq: u64);
    /// 分片激活阈值单位: shard i (i>0) 在积压 ≥ i × 此值时激活
    const ACTIVATION_UNIT: usize;
    /// 单次 drain 上限
    const MAX_DRAIN: usize;
    /// 快照语义流 (如波形): 每批是全量快照, 分片并行无意义 — 仅 shard 0 工作,
    /// 其余分片保持休眠 (不记录日志, 避免激活/休眠抖动刷屏)
    const SNAPSHOT: bool = false;
}

/// 统一分片推送循环 — 所有数据流共用
///
/// 由 subscribe_* 命令按分片 spawn; 取消信号或 Channel 关闭时退出。
#[allow(clippy::too_many_arguments)]
pub async fn sharded_stream_loop<S: StreamSource>(
    name: String,
    source: Arc<Mutex<S>>,
    on_event: Channel<S::Batch>,
    shard_idx: usize,
    seq: Arc<AtomicU64>,
    interval: Duration,
    min_batch: usize,
    cancel_rx: oneshot::Receiver<()>,
) {
    let channel_id = on_event.id();
    let rate = AdaptiveRate::new(
        Duration::from_millis(16),
        interval.max(Duration::from_millis(100)),
    );
    let log_name = name.clone();
    let mut was_active = shard_idx == 0;
    adaptive_channel_loop(
        &name,
        channel_id,
        on_event,
        rate,
        move || {
            let mut src = source.lock();
            let backlog = src.backlog();
            // 自动扩缩容: 积压不足时高分片休眠 (状态变化时输出日志)
            // 快照流 (SNAPSHOT=true) 永远只有 shard 0 工作
            let active = shard_idx == 0 || (!S::SNAPSHOT && backlog >= shard_idx * S::ACTIVATION_UNIT);
            if active != was_active {
                log::debug!(
                    "{} {} (积压 {}, 阈值 {})",
                    log_name,
                    if active { "激活" } else { "休眠" },
                    backlog,
                    shard_idx * S::ACTIVATION_UNIT
                );
                was_active = active;
            }
            if !active {
                return None;
            }
            let mut batch = src.drain(backlog.clamp(min_batch, S::MAX_DRAIN))?;
            // seq 在源锁内分配, 保证全局单调且与 drain 顺序一致
            S::set_seq(&mut batch, seq.fetch_add(1, Ordering::Relaxed));
            Some(batch)
        },
        cancel_rx,
    )
    .await;
}

/// [`join_or_create_group`] 的返回打包: (共享流源, 组级 seq, 本分片序号, 组 id)
pub type GroupMembership<S> = (Arc<Mutex<S>>, Arc<AtomicU64>, usize, String);

/// 创建/加入流订阅组
///
/// - `group_id = None`: 以本 channel 为 shard 0 建组, `make_source` 创建组共享流源,
///   返回组 id (= 首个 channel id 字符串)
/// - `group_id = Some`: 作为新 shard 加入, 流源实例按类型 downcast 取回 (类型不符报错)
/// - `max_shards`: 组最大分片数 (来自 PipelineConfig::max_stream_shards)
pub fn join_or_create_group<S, F>(
    groups: &Arc<Mutex<HashMap<String, StreamGroupState>>>,
    group_id: Option<String>,
    channel_id: u32,
    max_shards: usize,
    make_source: F,
) -> Result<GroupMembership<S>>
where
    S: StreamSource,
    F: FnOnce() -> S,
{
    let mut map = groups.lock();
    match group_id {
        None => {
            let key = channel_id.to_string();
            let source = Arc::new(Mutex::new(make_source()));
            let seq = Arc::new(AtomicU64::new(0));
            map.insert(
                key.clone(),
                StreamGroupState {
                    seq: seq.clone(),
                    shards: 1,
                    source: source.clone(),
                },
            );
            Ok((source, seq, 0, key))
        }
        Some(key) => {
            let g = map
                .get_mut(&key)
                .ok_or_else(|| Error::Config(format!("流订阅组不存在: {}", key)))?;
            if g.shards >= max_shards {
                return Err(Error::Config(format!(
                    "流订阅组 {} 已满 ({} 个分片)",
                    key, max_shards
                )));
            }
            g.shards += 1;
            let source = g
                .source
                .clone()
                .downcast::<Mutex<S>>()
                .map_err(|_| Error::Config(format!("流订阅组类型不匹配: {}", key)))?;
            Ok((source, g.seq.clone(), g.shards - 1, key))
        }
    }
}

/// 分片退出时调用: 组计数 -1, 空组移除
pub fn leave_group(groups: &Arc<Mutex<HashMap<String, StreamGroupState>>>, key: &str) {
    let mut map = groups.lock();
    if let Some(g) = map.get_mut(key) {
        g.shards = g.shards.saturating_sub(1);
        if g.shards == 0 {
            map.remove(key);
        }
    }
}

// ============ 各数据流的 Source 实现 ============

/// 原始字节流 (全局 RAWDATA / RawData 节点旁路共用) — 增量 drain
pub struct RawDataSource {
    collector: Arc<Mutex<RawDataCollector>>,
}

impl RawDataSource {
    pub fn new(collector: Arc<Mutex<RawDataCollector>>) -> Self {
        Self { collector }
    }
}

impl StreamSource for RawDataSource {
    type Batch = RawDataBatch;

    fn backlog(&mut self) -> usize {
        self.collector.lock().stored_bytes()
    }

    fn drain(&mut self, max: usize) -> Option<Self::Batch> {
        // 锁内仅移动 Vec (drain_raw), base64 编码在锁外 (into_batch) —
        // 编码耗时与数据量成正比, 在锁内会阻塞 data_loop 的 push_chunk
        let raw = { self.collector.lock().drain_raw(max) };
        if raw.chunks.is_empty() {
            None
        } else {
            Some(raw.into_batch())
        }
    }

    fn set_seq(batch: &mut Self::Batch, seq: u64) {
        batch.seq = seq;
    }

    const ACTIVATION_UNIT: usize = 256 * 1024;
    const MAX_DRAIN: usize = 1024 * 1024;
}

/// CAN 帧流 — 组内游标增量读取 (游标起点回溯 max_items, 订阅即可见近期历史)
pub struct CanStreamSource {
    buffer: Arc<Mutex<CanBuffer>>,
    cursor: u64,
}

impl CanStreamSource {
    pub fn new(buffer: Arc<Mutex<CanBuffer>>, max_items: usize) -> Self {
        let cursor = {
            let buf = buffer.lock();
            buf.version().saturating_sub(max_items as u64)
        };
        Self { buffer, cursor }
    }
}

impl StreamSource for CanStreamSource {
    type Batch = CanFrameBatch;

    fn backlog(&mut self) -> usize {
        let buf = self.buffer.lock();
        usize::try_from(buf.version().saturating_sub(self.cursor)).unwrap_or(usize::MAX)
    }

    fn drain(&mut self, max: usize) -> Option<Self::Batch> {
        let buf = self.buffer.lock();
        let (items, new_cursor, _dropped) = buf.drain_from(self.cursor, max);
        self.cursor = new_cursor;
        if items.is_empty() {
            None
        } else {
            Some(CanFrameBatch { seq: 0, frames: items })
        }
    }

    fn set_seq(batch: &mut Self::Batch, seq: u64) {
        batch.seq = seq;
    }

    const ACTIVATION_UNIT: usize = 1000;
    const MAX_DRAIN: usize = 2000;
}

/// 逻辑采样流 — 组内游标增量读取
pub struct LogicStreamSource {
    buffer: Arc<Mutex<LogicBuffer>>,
    cursor: u64,
}

impl LogicStreamSource {
    pub fn new(buffer: Arc<Mutex<LogicBuffer>>, max_items: usize) -> Self {
        let cursor = {
            let buf = buffer.lock();
            buf.version().saturating_sub(max_items as u64)
        };
        Self { buffer, cursor }
    }
}

impl StreamSource for LogicStreamSource {
    type Batch = LogicSampleBatch;

    fn backlog(&mut self) -> usize {
        let buf = self.buffer.lock();
        usize::try_from(buf.version().saturating_sub(self.cursor)).unwrap_or(usize::MAX)
    }

    fn drain(&mut self, max: usize) -> Option<Self::Batch> {
        let buf = self.buffer.lock();
        let (items, new_cursor, _dropped) = buf.drain_from(self.cursor, max);
        self.cursor = new_cursor;
        if items.is_empty() {
            None
        } else {
            Some(LogicSampleBatch { seq: 0, samples: items })
        }
    }

    fn set_seq(batch: &mut Self::Batch, seq: u64) {
        batch.seq = seq;
    }

    const ACTIVATION_UNIT: usize = 2000;
    const MAX_DRAIN: usize = 4000;
}

/// 解码事件流 — 组内游标增量读取
pub struct DecodedStreamSource {
    buffer: Arc<Mutex<DecodedBuffer>>,
    cursor: u64,
}

impl DecodedStreamSource {
    pub fn new(buffer: Arc<Mutex<DecodedBuffer>>, max_items: usize) -> Self {
        let cursor = {
            let buf = buffer.lock();
            buf.version().saturating_sub(max_items as u64)
        };
        Self { buffer, cursor }
    }
}

impl StreamSource for DecodedStreamSource {
    type Batch = DecodedEventBatch;

    fn backlog(&mut self) -> usize {
        let buf = self.buffer.lock();
        usize::try_from(buf.version().saturating_sub(self.cursor)).unwrap_or(usize::MAX)
    }

    fn drain(&mut self, max: usize) -> Option<Self::Batch> {
        let buf = self.buffer.lock();
        let (items, new_cursor, _dropped) = buf.drain_from(self.cursor, max);
        self.cursor = new_cursor;
        if items.is_empty() {
            None
        } else {
            Some(DecodedEventBatch { seq: 0, events: items })
        }
    }

    fn set_seq(batch: &mut Self::Batch, seq: u64) {
        batch.seq = seq;
    }

    const ACTIVATION_UNIT: usize = 500;
    const MAX_DRAIN: usize = 1000;
}

/// 波形流 — 快照语义 (唯一非增量流): version 变化即推送最新窗口,
/// 前端按 "最新 seq 胜出" 丢弃乱序旧快照
pub struct WaveformSource {
    buffer: Arc<Mutex<DataBuffer>>,
    last_version: u64,
}

impl WaveformSource {
    pub fn new(buffer: Arc<Mutex<DataBuffer>>) -> Self {
        Self {
            buffer,
            last_version: 0,
        }
    }
}

impl StreamSource for WaveformSource {
    type Batch = WaveformWindow;

    fn backlog(&mut self) -> usize {
        let buf = self.buffer.lock();
        usize::try_from(buf.version().saturating_sub(self.last_version)).unwrap_or(usize::MAX)
    }

    fn drain(&mut self, max: usize) -> Option<Self::Batch> {
        let buf = self.buffer.lock();
        let version = buf.version();
        if version == self.last_version {
            return None;
        }
        let pts = buf.point_count().min(max);
        let window = buf.get_recent(pts);
        self.last_version = version;
        Some(window)
    }

    fn set_seq(batch: &mut Self::Batch, seq: u64) {
        batch.seq = seq;
    }

    const ACTIVATION_UNIT: usize = 200;
    const MAX_DRAIN: usize = 5000;
    const SNAPSHOT: bool = true;
}
