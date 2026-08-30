//! Actor 化的数据 Topic 总线。
//!
//! 数据平面只负责发布有效样本；每个 Topic Actor 独占自己的环形历史、序号和
//! 订阅广播器。这样采集/图求值热路径不再与任意前端订阅共享数据锁。

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};

const COMMAND_CAPACITY: usize = 256;
const PREVIEW_CAPACITY: usize = 8;
const ESTIMATED_TOPIC_COUNT: usize = 64;
const SAMPLE_BYTES: usize = 24;

/// 一个数值端口的稳定标识。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TopicKey {
    pub source_node_id: String,
    pub source_handle: String,
}

impl TopicKey {
    #[must_use]
    pub fn new(source_node_id: impl Into<String>, source_handle: impl Into<String>) -> Self {
        Self {
            source_node_id: source_node_id.into(),
            source_handle: source_handle.into(),
        }
    }
}

/// Topic 当前数据状态。状态与样本值正交，零值不会被误判为断开。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SampleStatus {
    Waiting,
    Live,
    Disconnected,
    ChannelOutOfRange { requested: usize, available: usize },
    Overrun { lost_samples: u64 },
}

/// 单个有效样本。无效/缺失数据不会构造此类型。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub sequence: u64,
    pub timestamp_us: u64,
    pub value: f64,
}

/// Actor 向订阅者发布的有序批次。
#[derive(Debug, Clone)]
pub struct SampleBatch {
    pub topic: TopicKey,
    pub sequence: u64,
    pub samples: Arc<[Sample]>,
    pub status: SampleStatus,
    pub preview_skipped: u64,
    pub retention_evicted: u64,
    pub ingress_dropped: u64,
}

/// 自动运行模式的安全上限。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeLimits {
    pub max_workers: usize,
    pub memory_budget_mb: usize,
    pub preview_fps_limit: u32,
    pub preview_bandwidth_mb_per_sec: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_workers: 8,
            memory_budget_mb: 256,
            preview_fps_limit: 60,
            preview_bandwidth_mb_per_sec: 8,
        }
    }
}

/// 可从前端查询的累计健康信息。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeHealth {
    pub active_topics: u64,
    pub published_samples: u64,
    pub preview_skipped: u64,
    pub retention_evicted: u64,
    pub ingress_dropped: u64,
    pub last_ack_sequence: u64,
    pub recommended_interval_ms: u64,
}

#[derive(Default)]
struct Metrics {
    active_topics: AtomicU64,
    published_samples: AtomicU64,
    preview_skipped: AtomicU64,
    retention_evicted: AtomicU64,
    ingress_dropped: AtomicU64,
    last_ack_sequence: AtomicU64,
    recommended_interval_ms: AtomicU64,
}

impl Metrics {
    fn snapshot(&self) -> RuntimeHealth {
        RuntimeHealth {
            active_topics: self.active_topics.load(Ordering::Relaxed),
            published_samples: self.published_samples.load(Ordering::Relaxed),
            preview_skipped: self.preview_skipped.load(Ordering::Relaxed),
            retention_evicted: self.retention_evicted.load(Ordering::Relaxed),
            ingress_dropped: self.ingress_dropped.load(Ordering::Relaxed),
            last_ack_sequence: self.last_ack_sequence.load(Ordering::Relaxed),
            recommended_interval_ms: self.recommended_interval_ms.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone)]
struct TopicHandle {
    ingress: mpsc::Sender<PublishCommand>,
    control: mpsc::UnboundedSender<ControlCommand>,
    subscribers: Arc<AtomicU64>,
    overrun_pending: Arc<AtomicBool>,
}

struct PublishCommand {
    timestamps: Arc<[u64]>,
    values: Arc<[f64]>,
}

enum ControlCommand {
    SetStatus(SampleStatus),
    Subscribe {
        replay_limit: usize,
        reply: oneshot::Sender<broadcast::Receiver<Arc<SampleBatch>>>,
    },
    Ack {
        sequence: u64,
        buffered_bytes: usize,
        render_ms: f64,
    },
    PreviewSkipped(u64),
}

struct Inner {
    topics: Mutex<HashMap<TopicKey, TopicHandle>>,
    subscriptions: Mutex<HashMap<u32, TopicKey>>,
    limits: RwLock<RuntimeLimits>,
    metrics: Arc<Metrics>,
}

/// 克隆成本很低的进程内数据总线。
#[derive(Clone)]
pub struct DataBus {
    inner: Arc<Inner>,
}

impl Default for DataBus {
    fn default() -> Self {
        Self::new(RuntimeLimits::default())
    }
}

impl DataBus {
    #[must_use]
    pub fn new(limits: RuntimeLimits) -> Self {
        Self {
            inner: Arc::new(Inner {
                topics: Mutex::new(HashMap::new()),
                subscriptions: Mutex::new(HashMap::new()),
                limits: RwLock::new(limits),
                metrics: Arc::new(Metrics::default()),
            }),
        }
    }

    pub fn set_limits(&self, limits: RuntimeLimits) {
        *self.inner.limits.write() = limits;
    }

    #[must_use]
    pub fn limits(&self) -> RuntimeLimits {
        *self.inner.limits.read()
    }

    #[must_use]
    pub fn health(&self) -> RuntimeHealth {
        self.inner.metrics.snapshot()
    }

    /// 某端口存在订阅时返回 true，供热路径避免构造无人消费的派生批次。
    #[must_use]
    pub fn is_active(&self, key: &TopicKey) -> bool {
        self.inner
            .topics
            .lock()
            .get(key)
            .is_some_and(|topic| topic.subscribers.load(Ordering::Relaxed) > 0)
    }

    #[must_use]
    pub fn active_topics_for_source(&self, source_node_id: &str) -> Vec<TopicKey> {
        self.inner
            .topics
            .lock()
            .iter()
            .filter(|(key, topic)| {
                key.source_node_id == source_node_id
                    && topic.subscribers.load(Ordering::Relaxed) > 0
            })
            .map(|(key, _)| key.clone())
            .collect()
    }

    pub fn set_source_status(&self, source_node_id: &str, status: SampleStatus) {
        for key in self.active_topics_for_source(source_node_id) {
            self.set_status(key, status.clone());
        }
    }

    pub fn record_ack(&self, sequence: u64, buffered_bytes: usize, render_ms: f64) {
        self.inner
            .metrics
            .last_ack_sequence
            .store(sequence, Ordering::Relaxed);
        let limits = self.limits();
        let minimum = 1_000_u64 / u64::from(limits.preview_fps_limit.max(1));
        let overloaded = render_ms > 16.0
            || buffered_bytes
                > limits
                    .preview_bandwidth_mb_per_sec
                    .saturating_mul(1024 * 1024);
        self.inner.metrics.recommended_interval_ms.store(
            if overloaded {
                minimum.saturating_mul(2)
            } else {
                minimum
            },
            Ordering::Relaxed,
        );
    }

    fn samples_per_topic(&self) -> usize {
        let bytes =
            self.limits().memory_budget_mb.saturating_mul(1024 * 1024) / ESTIMATED_TOPIC_COUNT;
        (bytes / SAMPLE_BYTES).max(4_096)
    }

    fn ensure_topic(&self, key: &TopicKey) -> Option<TopicHandle> {
        let existing = self.inner.topics.lock().get(key).cloned();
        if let Some(handle) = existing {
            return Some(handle);
        }
        let runtime = match tokio::runtime::Handle::try_current() {
            Ok(runtime) => runtime,
            Err(error) => {
                log::error!("无法启动数据 Topic Actor: {error}");
                return None;
            }
        };
        let (ingress, ingress_rx) = mpsc::channel(COMMAND_CAPACITY);
        let (control, control_rx) = mpsc::unbounded_channel();
        let handle = TopicHandle {
            ingress,
            control,
            subscribers: Arc::new(AtomicU64::new(0)),
            overrun_pending: Arc::new(AtomicBool::new(false)),
        };
        let mut topics = self.inner.topics.lock();
        if let Some(existing) = topics.get(key).cloned() {
            return Some(existing);
        }
        topics.insert(key.clone(), handle.clone());
        runtime.spawn(run_topic(
            key.clone(),
            ingress_rx,
            control_rx,
            self.samples_per_topic(),
            self.inner.metrics.clone(),
            self.limits(),
        ));
        Some(handle)
    }

    /// 非阻塞发布有效样本。Topic 队列溢出会被精确计数并转为 Overrun 状态。
    pub fn publish_samples(&self, key: TopicKey, timestamps: Arc<[u64]>, values: Arc<[f64]>) {
        if timestamps.is_empty() || timestamps.len() != values.len() {
            return;
        }
        let Some(topic) = self.ensure_topic(&key) else {
            return;
        };
        let count = values.len() as u64;
        if topic
            .ingress
            .try_send(PublishCommand { timestamps, values })
            .is_err()
        {
            self.inner
                .metrics
                .ingress_dropped
                .fetch_add(count, Ordering::Relaxed);
            if !topic.overrun_pending.swap(true, Ordering::Relaxed) {
                let total = self.inner.metrics.ingress_dropped.load(Ordering::Relaxed);
                let _ = topic
                    .control
                    .send(ControlCommand::SetStatus(SampleStatus::Overrun {
                        lost_samples: total,
                    }));
            }
        } else {
            topic.overrun_pending.store(false, Ordering::Relaxed);
        }
    }

    pub fn set_status(&self, key: TopicKey, status: SampleStatus) {
        if let Some(topic) = self.ensure_topic(&key) {
            let _ = topic.control.send(ControlCommand::SetStatus(status));
        }
    }

    pub async fn subscribe(
        &self,
        key: TopicKey,
        replay_limit: usize,
    ) -> Option<broadcast::Receiver<Arc<SampleBatch>>> {
        let topic = self.ensure_topic(&key)?;
        let (reply, response) = oneshot::channel();
        topic
            .control
            .send(ControlCommand::Subscribe {
                replay_limit,
                reply,
            })
            .ok()?;
        let receiver = response.await.ok()?;
        if topic.subscribers.fetch_add(1, Ordering::Relaxed) == 0 {
            self.inner
                .metrics
                .active_topics
                .fetch_add(1, Ordering::Relaxed);
        }
        Some(receiver)
    }

    pub fn unsubscribe(&self, key: &TopicKey) {
        let topic = self.inner.topics.lock().get(key).cloned();
        if let Some(topic) = topic {
            let previous =
                topic
                    .subscribers
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                        Some(count.saturating_sub(1))
                    });
            if previous == Ok(1) {
                self.inner
                    .metrics
                    .active_topics
                    .fetch_sub(1, Ordering::Relaxed);
            }
        }
    }

    pub fn register_subscription(&self, subscription_id: u32, key: TopicKey) {
        self.inner.subscriptions.lock().insert(subscription_id, key);
    }

    pub fn unregister_subscription(&self, subscription_id: u32) {
        let key = self.inner.subscriptions.lock().remove(&subscription_id);
        if let Some(key) = key {
            self.unsubscribe(&key);
        }
    }

    pub fn ack_subscription(
        &self,
        subscription_id: u32,
        sequence: u64,
        buffered_bytes: usize,
        render_ms: f64,
    ) {
        if self
            .inner
            .subscriptions
            .lock()
            .contains_key(&subscription_id)
        {
            self.record_ack(sequence, buffered_bytes, render_ms);
        }
    }

    pub fn record_preview_skipped(&self, key: &TopicKey, skipped: u64) {
        let topic = self.inner.topics.lock().get(key).cloned();
        if let Some(topic) = topic {
            let _ = topic.control.send(ControlCommand::PreviewSkipped(skipped));
        }
    }

    pub fn ack(&self, key: &TopicKey, sequence: u64, buffered_bytes: usize, render_ms: f64) {
        let topic = self.inner.topics.lock().get(key).cloned();
        if let Some(topic) = topic {
            let _ = topic.control.send(ControlCommand::Ack {
                sequence,
                buffered_bytes,
                render_ms,
            });
        }
    }
}

async fn run_topic(
    key: TopicKey,
    mut ingress: mpsc::Receiver<PublishCommand>,
    mut control: mpsc::UnboundedReceiver<ControlCommand>,
    capacity: usize,
    metrics: Arc<Metrics>,
    limits: RuntimeLimits,
) {
    let (events, _) = broadcast::channel(PREVIEW_CAPACITY);
    let mut history = VecDeque::<Sample>::with_capacity(capacity);
    let mut next_sequence = 0_u64;
    let mut event_sequence = 0_u64;
    let mut status = SampleStatus::Waiting;
    let mut retention_evicted = 0_u64;
    let mut preview_skipped = 0_u64;

    loop {
        tokio::select! {
            biased;
            command = control.recv() => {
                let Some(command) = command else { break };
                match command {
                    ControlCommand::SetStatus(next) => {
                        if next == SampleStatus::Disconnected {
                            // 断开是生命周期屏障：丢弃断开前尚未处理的预览批次，
                            // 防止它们随后把状态重新覆盖成 Live，并淹没断开事件。
                            while ingress.try_recv().is_ok() {
                                preview_skipped = preview_skipped.saturating_add(1);
                                metrics.preview_skipped.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        status = next;
                        let _ = events.send(Arc::new(SampleBatch {
                            topic: key.clone(),
                            sequence: event_sequence,
                            samples: Arc::from([]),
                            status: status.clone(),
                            preview_skipped,
                            retention_evicted,
                            ingress_dropped: metrics.ingress_dropped.load(Ordering::Relaxed),
                        }));
                        event_sequence = event_sequence.wrapping_add(1);
                    }
                    ControlCommand::Subscribe { replay_limit, reply } => {
                        let receiver = events.subscribe();
                        let _ = reply.send(receiver);
                        if !history.is_empty() || status != SampleStatus::Waiting {
                            let start = history.len().saturating_sub(replay_limit);
                            let recent: Arc<[Sample]> = history
                                .iter()
                                .skip(start)
                                .copied()
                                .collect::<Vec<_>>()
                                .into();
                            let _ = events.send(Arc::new(SampleBatch {
                                topic: key.clone(),
                                sequence: event_sequence,
                                samples: recent,
                                status: status.clone(),
                                preview_skipped,
                                retention_evicted,
                                ingress_dropped: metrics.ingress_dropped.load(Ordering::Relaxed),
                            }));
                            event_sequence = event_sequence.wrapping_add(1);
                        }
                    }
                    ControlCommand::Ack {
                        sequence,
                        buffered_bytes,
                        render_ms,
                    } => {
                        metrics.last_ack_sequence.store(sequence, Ordering::Relaxed);
                        let min_interval = 1_000_u64 / u64::from(limits.preview_fps_limit.max(1));
                        let overloaded = render_ms > 16.0
                            || buffered_bytes
                                > limits
                                    .preview_bandwidth_mb_per_sec
                                    .saturating_mul(1024 * 1024);
                        let interval = if overloaded {
                            Duration::from_millis(min_interval).saturating_mul(2)
                        } else {
                            Duration::from_millis(min_interval)
                        };
                        metrics.recommended_interval_ms.store(
                            u64::try_from(interval.as_millis()).unwrap_or(u64::MAX),
                            Ordering::Relaxed,
                        );
                    }
                    ControlCommand::PreviewSkipped(skipped) => {
                        preview_skipped = preview_skipped.saturating_add(skipped);
                        metrics
                            .preview_skipped
                            .fetch_add(skipped, Ordering::Relaxed);
                    }
                }
            }
            command = ingress.recv() => {
                let Some(PublishCommand { timestamps, values }) = command else { break };
                let mut samples = Vec::with_capacity(values.len());
                let mut evicted_now = 0_u64;
                for (&timestamp_us, &value) in timestamps.iter().zip(values.iter()) {
                    let sample = Sample {
                        sequence: next_sequence,
                        timestamp_us,
                        value,
                    };
                    next_sequence = next_sequence.wrapping_add(1);
                    if history.len() == capacity {
                        history.pop_front();
                        retention_evicted = retention_evicted.saturating_add(1);
                        evicted_now = evicted_now.saturating_add(1);
                    }
                    history.push_back(sample);
                    samples.push(sample);
                }
                status = SampleStatus::Live;
                metrics
                    .published_samples
                    .fetch_add(samples.len() as u64, Ordering::Relaxed);
                metrics
                    .retention_evicted
                    .fetch_add(evicted_now, Ordering::Relaxed);
                let batch = Arc::new(SampleBatch {
                    topic: key.clone(),
                    sequence: event_sequence,
                    samples: samples.into(),
                    status: status.clone(),
                    preview_skipped,
                    retention_evicted,
                    ingress_dropped: metrics.ingress_dropped.load(Ordering::Relaxed),
                });
                event_sequence = event_sequence.wrapping_add(1);
                if events.send(batch).is_err() {
                    preview_skipped = preview_skipped.saturating_add(1);
                    metrics.preview_skipped.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}

/// 数据平面批处理/worker 自动调节器。用迟滞避免负载边界反复扩缩。
#[derive(Debug, Clone)]
pub struct AdaptiveController {
    workers: usize,
    high_streak: u8,
    low_since: Option<std::time::Instant>,
    target_batch_bytes: usize,
    ewma_input_bytes_per_sec: u64,
    last_observed: std::time::Instant,
}

impl Default for AdaptiveController {
    fn default() -> Self {
        Self {
            workers: 1,
            high_streak: 0,
            low_since: None,
            target_batch_bytes: 64 * 1024,
            ewma_input_bytes_per_sec: 0,
            last_observed: std::time::Instant::now(),
        }
    }
}

impl AdaptiveController {
    pub fn observe(
        &mut self,
        queue_fill: f64,
        queue_age: Duration,
        service_time: Duration,
        input_bytes: usize,
        limits: RuntimeLimits,
    ) {
        let elapsed_us = u64::try_from(self.last_observed.elapsed().as_micros())
            .unwrap_or(u64::MAX)
            .max(1);
        self.last_observed = std::time::Instant::now();
        let current_rate = u64::try_from(input_bytes)
            .unwrap_or(u64::MAX)
            .saturating_mul(1_000_000)
            / elapsed_us;
        self.ewma_input_bytes_per_sec = if self.ewma_input_bytes_per_sec == 0 {
            current_rate
        } else {
            self.ewma_input_bytes_per_sec
                .saturating_mul(4)
                .saturating_add(current_rate)
                / 5
        };

        let max_workers = limits
            .max_workers
            .min(std::thread::available_parallelism().map_or(1, std::num::NonZero::get))
            .max(1);
        if queue_fill > 0.5 || queue_age > Duration::from_millis(10) {
            self.high_streak = self.high_streak.saturating_add(1);
            self.low_since = None;
            if self.high_streak >= 3 && self.workers < max_workers {
                self.workers += 1;
                self.high_streak = 0;
            }
        } else if queue_fill < 0.1 && queue_age < Duration::from_millis(2) {
            let since = self.low_since.get_or_insert_with(std::time::Instant::now);
            if since.elapsed() >= Duration::from_secs(2) && self.workers > 1 {
                self.workers -= 1;
                self.low_since = Some(std::time::Instant::now());
            }
        } else {
            self.high_streak = 0;
            self.low_since = None;
        }

        if service_time > Duration::from_millis(16) {
            self.target_batch_bytes = (self.target_batch_bytes / 2).max(16 * 1024);
        } else {
            let target_ms = if queue_fill > 0.5 || queue_age > Duration::from_millis(10) {
                16
            } else {
                8
            };
            let rate_target =
                usize::try_from(self.ewma_input_bytes_per_sec.saturating_mul(target_ms) / 1_000)
                    .unwrap_or(usize::MAX);
            let rate_target = rate_target.clamp(16 * 1024, 1024 * 1024);
            self.target_batch_bytes = (self.target_batch_bytes * 3 + rate_target) / 4;
        }
    }

    #[must_use]
    pub const fn workers(&self) -> usize {
        self.workers
    }

    #[must_use]
    pub const fn target_batch_bytes(&self) -> usize {
        self.target_batch_bytes
    }

    #[must_use]
    pub const fn ewma_input_bytes_per_sec(&self) -> u64 {
        self.ewma_input_bytes_per_sec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn zero_is_a_valid_sample_but_waiting_has_no_sample() {
        let bus = DataBus::default();
        let key = TopicKey::new("protocol", "ch3");
        let mut rx = bus.subscribe(key.clone(), 500).await.unwrap();
        bus.publish_samples(key, Arc::from([10]), Arc::from([0.0]));
        let batch = rx.recv().await.unwrap();
        assert_eq!(batch.status, SampleStatus::Live);
        assert!(batch.samples[0].value.abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn invalid_channel_status_does_not_fabricate_zero() {
        let bus = DataBus::default();
        let key = TopicKey::new("protocol", "ch9");
        let mut rx = bus.subscribe(key.clone(), 500).await.unwrap();
        bus.set_status(
            key,
            SampleStatus::ChannelOutOfRange {
                requested: 9,
                available: 4,
            },
        );
        let batch = rx.recv().await.unwrap();
        assert!(batch.samples.is_empty());
        assert!(matches!(
            batch.status,
            SampleStatus::ChannelOutOfRange { .. }
        ));
    }

    #[tokio::test]
    async fn topic_activity_tracks_subscription_lifetime() {
        let bus = DataBus::default();
        let key = TopicKey::new("protocol", "ch0");
        let _receiver = bus.subscribe(key.clone(), 500).await.unwrap();
        assert!(bus.is_active(&key));
        assert_eq!(bus.health().active_topics, 1);

        bus.register_subscription(42, key.clone());
        bus.ack_subscription(42, 7, 0, 1.0);
        assert_eq!(bus.health().last_ack_sequence, 7);

        bus.unregister_subscription(42);
        assert!(!bus.is_active(&key));
        assert_eq!(bus.health().active_topics, 0);
    }

    #[tokio::test]
    async fn replay_is_bounded_to_latest_samples() {
        let bus = DataBus::default();
        let key = TopicKey::new("protocol", "ch1");
        let mut first = bus.subscribe(key.clone(), 500).await.unwrap();
        bus.publish_samples(
            key.clone(),
            Arc::from([1, 2, 3, 4, 5]),
            Arc::from([1.0, 2.0, 3.0, 4.0, 5.0]),
        );
        let _ = first.recv().await.unwrap();
        bus.unsubscribe(&key);

        let mut replay = bus.subscribe(key, 2).await.unwrap();
        let batch = replay.recv().await.unwrap();
        assert_eq!(
            batch
                .samples
                .iter()
                .map(|sample| sample.value)
                .collect::<Vec<_>>(),
            vec![4.0, 5.0]
        );
    }

    #[tokio::test]
    async fn lifecycle_status_bypasses_saturated_ingress() {
        let bus = DataBus::default();
        let key = TopicKey::new("protocol", "ch2");
        let mut rx = bus.subscribe(key.clone(), 1).await.unwrap();
        let waiter = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(batch) if batch.status == SampleStatus::Disconnected => break batch,
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(error) => panic!("topic closed before disconnect status: {error}"),
                }
            }
        });
        for i in 0..u32::try_from(COMMAND_CAPACITY * 4).unwrap() {
            bus.publish_samples(
                key.clone(),
                Arc::from([u64::from(i)]),
                Arc::from([f64::from(i)]),
            );
        }
        bus.set_status(key, SampleStatus::Disconnected);

        let status = tokio::time::timeout(Duration::from_millis(100), waiter)
            .await
            .expect("disconnect status should not wait behind sample ingress")
            .unwrap();
        assert!(status.samples.is_empty());
    }
}
