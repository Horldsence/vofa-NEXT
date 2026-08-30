use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use app_state::AppState;
use buffer_raw::DirectionFilter;
use pipeline_bus::{RuntimeHealth, SampleBatch, SampleStatus, TopicKey};
use pipeline_dispatcher::filtered_sources::{
    FilteredCanStreamSource, FilteredDecodedStreamSource, FilteredLogicStreamSource,
    FilteredRawDataSource,
};
use pipeline_stream::{
    AdaptiveRate, CanStreamSource, DecodedStreamSource, LogicStreamSource, RawDataSource,
    StreamSource, WaveformSource,
};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::State;
use vofa_core::Result;

use crate::snapshot::spawn_snapshot;
use crate::{DisplayEvent, DisplayRequest, RawDataOrigin, SubscriptionInfo};

const BINARY_SCHEMA_VERSION: u16 = 1;
const SAMPLE_EVENT_KIND: u16 = 1;
const SAMPLE_HEADER_LEN: usize = 68;

fn direction(value: &str) -> DirectionFilter {
    match value.to_ascii_lowercase().as_str() {
        "rx" => DirectionFilter::Rx,
        "tx" => DirectionFilter::Tx,
        _ => DirectionFilter::All,
    }
}

fn send_json(channel: &Channel<InvokeResponseBody>, event: &DisplayEvent) -> bool {
    let Ok(json) = serde_json::to_string(event) else {
        log::error!("显示事件序列化失败");
        return false;
    };
    channel.send(InvokeResponseBody::Json(json)).is_ok()
}

const fn status_code(status: &SampleStatus) -> u16 {
    match status {
        SampleStatus::Waiting => 0,
        SampleStatus::Live => 1,
        SampleStatus::Disconnected => 2,
        SampleStatus::ChannelOutOfRange { .. } => 3,
        SampleStatus::Overrun { .. } => 4,
    }
}

/// VNDP v1 little-endian columnar sample envelope.
fn encode_samples(batch: &SampleBatch) -> Vec<u8> {
    let count = batch.samples.len();
    let validity_len = count.saturating_add(7) / 8;
    let payload_len = count.saturating_mul(16).saturating_add(validity_len);
    let mut bytes = Vec::with_capacity(SAMPLE_HEADER_LEN + payload_len);
    bytes.extend_from_slice(b"VNDP");
    bytes.extend_from_slice(&BINARY_SCHEMA_VERSION.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_EVENT_KIND.to_le_bytes());
    bytes.extend_from_slice(&status_code(&batch.status).to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&batch.sequence.to_le_bytes());
    bytes.extend_from_slice(
        &batch
            .samples
            .first()
            .map_or(0, |sample| sample.sequence)
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&u32::try_from(count).unwrap_or(u32::MAX).to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&batch.preview_skipped.to_le_bytes());
    bytes.extend_from_slice(&batch.retention_evicted.to_le_bytes());
    bytes.extend_from_slice(&batch.ingress_dropped.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(payload_len).unwrap_or(u32::MAX).to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(SAMPLE_HEADER_LEN).unwrap().to_le_bytes());
    for sample in batch.samples.iter() {
        bytes.extend_from_slice(&sample.timestamp_us.to_le_bytes());
    }
    for sample in batch.samples.iter() {
        bytes.extend_from_slice(&sample.value.to_le_bytes());
    }
    for byte_index in 0..validity_len {
        let remaining = count.saturating_sub(byte_index * 8);
        let valid_bits = remaining.min(8);
        bytes.push(if valid_bits == 8 {
            u8::MAX
        } else {
            (1_u8 << valid_bits) - 1
        });
    }
    bytes
}

async fn can_bitrate(state: &AppState, node_id: &str, override_bps: Option<u32>) -> u32 {
    if let Some(value) = override_bps.filter(|value| *value > 0) {
        return value;
    }
    let manager = state.transport.lock().await;
    match manager.config(node_id) {
        Some(vofa_core::TransportConfig::Slcan(config)) => config.can_bitrate.bps(),
        Some(vofa_core::TransportConfig::CandleLight(config)) => config.can_bitrate.bps(),
        _ => 500_000,
    }
}

fn spawn_stream<S, F>(
    state: &AppState,
    mut source: S,
    on_event: Channel<InvokeResponseBody>,
    interval: Duration,
    min_batch: usize,
    name: &'static str,
    map: F,
) where
    S: StreamSource,
    F: Fn(S::Batch) -> DisplayEvent + Send + 'static,
{
    let channel_id = on_event.id();
    let mut cancel = subscription::register_cancel(&state.subscriptions, channel_id);
    let subscriptions = state.subscriptions.clone();
    tokio::spawn(async move {
        let mut rate = AdaptiveRate::new(
            Duration::from_millis(16),
            interval.max(Duration::from_millis(100)),
        );
        let seq = AtomicU64::new(0);
        loop {
            tokio::select! {
                _ = &mut cancel => break,
                () = tokio::time::sleep(rate.current()) => {
                    let backlog = source.backlog();
                    let Some(mut batch) = source.drain(backlog.clamp(min_batch, S::MAX_DRAIN)) else {
                        rate.on_idle();
                        continue;
                    };
                    S::set_seq(&mut batch, seq.fetch_add(1, Ordering::Relaxed));
                    if !send_json(&on_event, &map(batch)) { break; }
                    rate.on_send();
                }
            }
        }
        subscription::remove_subscription(&subscriptions, channel_id);
        log::debug!("{name}订阅已结束: {channel_id}");
    });
}

async fn spawn_can_load(
    state: &AppState,
    node_id: &str,
    bitrate_bps: Option<u32>,
    on_event: Channel<InvokeResponseBody>,
    interval: Duration,
) {
    let bitrate = can_bitrate(state, node_id, bitrate_bps).await;
    let load_stats = state.can_load_stats.clone();
    let channel_id = on_event.id();
    let mut cancel = subscription::register_cancel(&state.subscriptions, channel_id);
    let subscriptions = state.subscriptions.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            tokio::select! {
                _ = &mut cancel => break,
                _ = ticker.tick() => {
                    let snapshot = {
                        let mut value = load_stats.lock();
                        value.sample_history(bitrate, vofa_core::now_us());
                        value.snapshot(bitrate)
                    };
                    if !send_json(&on_event, &DisplayEvent::CanLoad(snapshot)) { break; }
                }
            }
        }
        subscription::remove_subscription(&subscriptions, channel_id);
    });
}

async fn spawn_sample_stream(
    state: &AppState,
    key: TopicKey,
    on_event: Channel<InvokeResponseBody>,
    interval: Duration,
    max_items: usize,
) -> bool {
    let data_bus = state.data_plane.eval.data_bus.clone();
    let max_items = max_items.max(1);
    let Some(mut receiver) = data_bus.subscribe(key.clone(), max_items).await else {
        return false;
    };
    let channel_id = on_event.id();
    data_bus.register_subscription(channel_id, key.clone());
    let mut cancel = subscription::register_cancel(&state.subscriptions, channel_id);
    let subscriptions = state.subscriptions.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval.max(Duration::from_millis(1)));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut pending: Option<std::sync::Arc<SampleBatch>> = None;
        let mut stream_preview_skipped = 0_u64;
        let mut skipped_since_report = 0_u64;
        loop {
            tokio::select! {
                biased;
                _ = &mut cancel => break,
                _ = ticker.tick() => {
                    let Some(batch) = pending.take() else { continue };
                    let start = batch.samples.len().saturating_sub(max_items);
                    let limited = SampleBatch {
                        topic: batch.topic.clone(),
                        sequence: batch.sequence,
                        samples: batch.samples[start..].to_vec().into(),
                        status: batch.status.clone(),
                        preview_skipped: stream_preview_skipped.max(batch.preview_skipped),
                        retention_evicted: batch.retention_evicted,
                        ingress_dropped: batch.ingress_dropped,
                    };
                    if skipped_since_report > 0 {
                        data_bus.record_preview_skipped(&key, skipped_since_report);
                        skipped_since_report = 0;
                    }
                    if on_event.send(InvokeResponseBody::Raw(encode_samples(&limited))).is_err() {
                        break;
                    }
                }
                event = receiver.recv() => match event {
                    Ok(batch) => {
                        stream_preview_skipped = stream_preview_skipped.max(batch.preview_skipped);
                        if pending.replace(batch).is_some() {
                            stream_preview_skipped = stream_preview_skipped.saturating_add(1);
                            skipped_since_report = skipped_since_report.saturating_add(1);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        data_bus.record_preview_skipped(&key, skipped);
                        log::debug!("样本预览跳过 {skipped} 批: channel={channel_id}");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
        data_bus.unregister_subscription(channel_id);
        subscription::remove_subscription(&subscriptions, channel_id);
    });
    true
}

/// 统一显示订阅。每个逻辑订阅只使用一个 Tauri Channel。
#[tauri::command]
pub async fn subscribe_data(
    state: State<'_, AppState>,
    request: DisplayRequest,
    on_event: Channel<InvokeResponseBody>,
    interval_ms: Option<u64>,
    max_items: Option<usize>,
) -> Result<SubscriptionInfo> {
    let interval = Duration::from_millis(interval_ms.unwrap_or(33));
    let max_items = max_items.unwrap_or(1_000);
    let channel_id = on_event.id();
    let mut mode = "json";
    match request {
        snapshot @ (DisplayRequest::GraphOutputs
        | DisplayRequest::CustomInputs
        | DisplayRequest::StringOutputs
        | DisplayRequest::Spectrum) => spawn_snapshot(&state, snapshot, on_event, interval),
        DisplayRequest::PortSamples {
            source_node_id,
            source_handle,
        } => {
            mode = "binary";
            let limits = state.data_plane.eval.data_bus.limits();
            let fps = u64::from(limits.preview_fps_limit.max(1));
            let minimum_ms = 1_000_u64.saturating_add(fps - 1) / fps;
            let _ = spawn_sample_stream(
                &state,
                TopicKey::new(source_node_id, source_handle),
                on_event,
                interval.max(Duration::from_millis(minimum_ms)),
                max_items,
            )
            .await;
        }
        DisplayRequest::Waveform { source } => spawn_stream(
            &state,
            WaveformSource::new(state.data_plane.buffer_for(&source)),
            on_event,
            interval,
            max_items,
            "波形",
            DisplayEvent::Waveform,
        ),
        DisplayRequest::RawData {
            origin,
            direction: filter_direction,
            search,
        } => {
            let collector = match origin {
                RawDataOrigin::Transport(id) => state.data_plane.raw_collector_for(&id),
                RawDataOrigin::Decoder(id) => match state.decoder_raw_collectors.lock().get(&id) {
                    Some(value) => value.clone(),
                    None => {
                        return Ok(SubscriptionInfo {
                            subscription_id: channel_id,
                            schema_version: BINARY_SCHEMA_VERSION,
                            mode,
                        })
                    }
                },
            };
            if filter_direction.is_empty() && search.trim().is_empty() {
                spawn_stream(
                    &state,
                    RawDataSource::new(collector),
                    on_event,
                    interval,
                    max_items,
                    "原始数据",
                    DisplayEvent::RawData,
                );
            } else {
                spawn_stream(
                    &state,
                    FilteredRawDataSource::new(
                        collector,
                        direction(&filter_direction),
                        (!search.trim().is_empty()).then_some(search.as_str()),
                    ),
                    on_event,
                    interval,
                    max_items,
                    "过滤原始数据",
                    DisplayEvent::RawData,
                );
            }
        }
        DisplayRequest::CanFrames { filter } => match filter {
            Some(filter) => spawn_stream(
                &state,
                FilteredCanStreamSource::new(state.can_buffer.clone(), filter),
                on_event,
                interval,
                max_items,
                "过滤 CAN",
                DisplayEvent::CanFrames,
            ),
            None => spawn_stream(
                &state,
                CanStreamSource::new(state.can_buffer.clone(), max_items),
                on_event,
                interval,
                max_items,
                "CAN",
                DisplayEvent::CanFrames,
            ),
        },
        DisplayRequest::LogicSamples { filter } => match filter {
            Some(filter) => spawn_stream(
                &state,
                FilteredLogicStreamSource::new(state.logic_buffer.clone(), filter),
                on_event,
                interval,
                max_items,
                "过滤逻辑采样",
                DisplayEvent::LogicSamples,
            ),
            None => spawn_stream(
                &state,
                LogicStreamSource::new(state.logic_buffer.clone(), max_items),
                on_event,
                interval,
                max_items,
                "逻辑采样",
                DisplayEvent::LogicSamples,
            ),
        },
        DisplayRequest::DecodedEvents { filter } => match filter {
            Some(filter) => spawn_stream(
                &state,
                FilteredDecodedStreamSource::new(state.decoded_buffer.clone(), filter),
                on_event,
                interval,
                max_items,
                "过滤解码事件",
                DisplayEvent::DecodedEvents,
            ),
            None => spawn_stream(
                &state,
                DecodedStreamSource::new(state.decoded_buffer.clone(), max_items),
                on_event,
                interval,
                max_items,
                "解码事件",
                DisplayEvent::DecodedEvents,
            ),
        },
        DisplayRequest::CanLoad {
            node_id,
            bitrate_bps,
        } => spawn_can_load(&state, &node_id, bitrate_bps, on_event, interval).await,
    }
    Ok(SubscriptionInfo {
        subscription_id: channel_id,
        schema_version: BINARY_SCHEMA_VERSION,
        mode,
    })
}

#[tauri::command]
pub async fn unsubscribe_data(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    subscription::cancel_subscription(&state.subscriptions, channel_id);
    Ok(())
}

/// 前端消费反馈。目前以运行时 Topic 保存控制状态。
#[tauri::command]
pub fn ack_data(
    state: State<'_, AppState>,
    subscription_id: u32,
    sequence: u64,
    buffered_bytes: usize,
    render_ms: f64,
) -> Result<()> {
    state.data_plane.eval.data_bus.ack_subscription(
        subscription_id,
        sequence,
        buffered_bytes,
        render_ms,
    );
    Ok(())
}

#[tauri::command]
pub fn get_data_health(state: State<'_, AppState>) -> Result<RuntimeHealth> {
    Ok(state.data_plane.eval.data_bus.health())
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_state::AppState;
    use pipeline_bus::Sample;
    use std::sync::Arc;
    use std::time::Duration;
    use vofa_core::DataFrame;

    #[test]
    fn binary_sample_contract_has_stable_header_and_zero_value() {
        let batch = SampleBatch {
            topic: TopicKey::new("FireWater", "ch3"),
            sequence: 7,
            samples: Arc::from([Sample {
                sequence: 9,
                timestamp_us: 11,
                value: 0.0,
            }]),
            status: SampleStatus::Live,
            preview_skipped: 1,
            retention_evicted: 2,
            ingress_dropped: 3,
        };
        let bytes = encode_samples(&batch);
        assert_eq!(&bytes[..4], b"VNDP");
        assert_eq!(bytes.len(), SAMPLE_HEADER_LEN + 17);
        assert!(f64::from_le_bytes(bytes[76..84].try_into().unwrap()).abs() < f64::EPSILON);
        assert_eq!(bytes[84], 1);
    }

    #[tokio::test]
    async fn firewater_ch3_reaches_sample_topic_within_preview_budget() {
        let state = AppState::new();
        let key = TopicKey::new("firewater", "ch3");
        let mut receiver = state
            .data_plane
            .eval
            .data_bus
            .subscribe(key, 500)
            .await
            .unwrap();
        let frames = [
            DataFrame {
                timestamp: 10,
                channels: vec![1.0, 2.0, 3.0, 4.0],
            },
            DataFrame {
                timestamp: 11,
                channels: vec![2.0, 3.0, 4.0, 5.0],
            },
        ];
        pipeline_data_plane::data_plane::frame_dispatch::on_frames(
            &state.data_plane,
            "firewater",
            &frames,
        );
        let batch = tokio::time::timeout(Duration::from_millis(250), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(batch.status, SampleStatus::Live);
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
    async fn out_of_range_topic_reports_status_without_zero_sample() {
        let state = AppState::new();
        let key = TopicKey::new("firewater", "ch9");
        let mut receiver = state
            .data_plane
            .eval
            .data_bus
            .subscribe(key, 500)
            .await
            .unwrap();
        pipeline_data_plane::data_plane::frame_dispatch::on_frames(
            &state.data_plane,
            "firewater",
            &[DataFrame {
                timestamp: 10,
                channels: vec![1.0, 2.0, 3.0, 4.0],
            }],
        );
        let batch = receiver.recv().await.unwrap();
        assert!(batch.samples.is_empty());
        assert_eq!(
            batch.status,
            SampleStatus::ChannelOutOfRange {
                requested: 9,
                available: 4,
            }
        );
    }
}
