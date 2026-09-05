//! Topic Actor — 每话题独占的历史环形缓冲 / 序号 / 订阅广播器

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::types::{RuntimeHealth, RuntimeLimits, Sample, SampleBatch, SampleStatus, TopicKey};

pub const COMMAND_CAPACITY: usize = 256;
const PREVIEW_CAPACITY: usize = 8;
pub const ESTIMATED_TOPIC_COUNT: usize = 64;
pub const SAMPLE_BYTES: usize = 24;

#[derive(Default)]
pub struct Metrics {
    pub active_topics: AtomicU64,
    pub published_samples: AtomicU64,
    pub preview_skipped: AtomicU64,
    retention_evicted: AtomicU64,
    pub ingress_dropped: AtomicU64,
    pub last_ack_sequence: AtomicU64,
    pub recommended_interval_ms: AtomicU64,
}

impl Metrics {
    pub fn snapshot(&self) -> RuntimeHealth {
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

pub struct PublishCommand {
    pub timestamps: Arc<[u64]>,
    pub values: Arc<[f64]>,
}

pub enum ControlCommand {
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

pub async fn run_topic(
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
