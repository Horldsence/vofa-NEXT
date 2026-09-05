use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use app_state::AppState;
use buffer_raw::DirectionFilter;
use data_bus::{RuntimeHealth, TopicKey};
use dispatcher::filtered_sources::{
    FilteredCanStreamSource, FilteredDecodedStreamSource, FilteredLogicStreamSource,
    FilteredRawDataSource,
};
use stream::{
    AdaptiveRate, CanStreamSource, DecodedStreamSource, LogicStreamSource, RawDataSource,
    StreamSource, WaveformSource,
};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::State;
use vofa_core::Result;

use crate::snapshot::spawn_snapshot;
use crate::{DisplayEvent, DisplayRequest, RawDataOrigin, SubscriptionInfo};

mod binary;
mod envelope_stream;
mod measurement_stream;
mod sample_stream;

use binary::BINARY_SCHEMA_VERSION;
use envelope_stream::spawn_envelope_stream;
use measurement_stream::spawn_measurement_stream;
use sample_stream::spawn_sample_stream;

/// 请求间隔是发送下限，不是空闲退避上限；概览 100ms 不能在有数据时加速到 16ms。
pub fn display_rate(interval: Duration, fps_limit: u32) -> AdaptiveRate {
    let fps = u64::from(fps_limit.max(1));
    let fps_interval = Duration::from_millis(1_000_u64.div_ceil(fps));
    let min = interval.max(fps_interval).max(Duration::from_millis(16));
    AdaptiveRate::new(min, min.max(Duration::from_millis(100)))
}

fn direction(value: &str) -> DirectionFilter {
    match value.to_ascii_lowercase().as_str() {
        "rx" => DirectionFilter::Rx,
        "tx" => DirectionFilter::Tx,
        _ => DirectionFilter::All,
    }
}

pub fn send_json(channel: &Channel<InvokeResponseBody>, event: &DisplayEvent) -> bool {
    let Ok(json) = serde_json::to_string(event) else {
        log::error!("显示事件序列化失败");
        return false;
    };
    channel.send(InvokeResponseBody::Json(json)).is_ok()
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

/// JSON 事件编码器 — 低频流 (原始数据/CAN/逻辑/解码事件) 沿用 JSON 联合事件
fn json_event<E, F>(map: F) -> impl Fn(E) -> InvokeResponseBody + Send + 'static
where
    E: serde::Serialize,
    F: Fn(E) -> DisplayEvent + Send + 'static,
{
    move |event| {
        let Ok(json) = serde_json::to_string(&map(event)) else {
            log::error!("显示事件序列化失败");
            return InvokeResponseBody::Json("{}".into());
        };
        InvokeResponseBody::Json(json)
    }
}

fn spawn_stream<S, F>(
    state: &AppState,
    mut source: S,
    on_event: Channel<InvokeResponseBody>,
    interval: Duration,
    min_batch: usize,
    name: &'static str,
    encode: F,
) where
    S: StreamSource,
    F: Fn(S::Batch) -> InvokeResponseBody + Send + 'static,
{
    let channel_id = on_event.id();
    let mut cancel = subscription::register_cancel(&state.subscriptions, channel_id);
    let subscriptions = state.subscriptions.clone();
    let mut rate = display_rate(
        interval,
        state.data_plane.eval.data_bus.limits().preview_fps_limit,
    );
    tokio::spawn(async move {
        let seq = AtomicU64::new(0);
        loop {
            tokio::select! {
                _ = &mut cancel => break,
                () = tokio::time::sleep(rate.current()) => {
                    let backlog = source.backlog();
                    // 显示点数预算由调用方传入；即使未来某个源给出超过自身硬上限的
                    // 预算，也不能让 usize::clamp(min > max) 使订阅任务 panic。
                    let drain_size = stream::bounded_drain_size(
                        backlog,
                        min_batch,
                        S::MAX_DRAIN,
                    );
                    let Some(mut batch) = source.drain(drain_size) else {
                        rate.on_idle();
                        continue;
                    };
                    S::set_seq(&mut batch, seq.fetch_add(1, Ordering::Relaxed));
                    if on_event.send(encode(batch)).is_err() { break; }
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
        DisplayRequest::WaveformEnvelope { source, columns } => {
            mode = "binary";
            let columns = usize::try_from(columns).unwrap_or(2048).clamp(16, 4096);
            spawn_envelope_stream(
                &state,
                state.data_plane.buffer_for(&source),
                columns,
                on_event,
                interval,
            );
        }
        DisplayRequest::Measurements {
            source,
            window_ms,
            derived,
        } => {
            spawn_measurement_stream(&state, &source, window_ms, derived, on_event, interval);
        }
        DisplayRequest::Waveform {
            source,
            start_ms,
            end_ms,
            selection,
        } => {
            let buffer = state.data_plane.buffer_for(&source);
            let waveform_source = match (start_ms, end_ms) {
                (Some(start_ms), Some(end_ms)) => WaveformSource::with_view(
                    buffer,
                    stream::WaveformViewSpec {
                        start_ms,
                        end_ms,
                        selection,
                    },
                ),
                _ => WaveformSource::new(buffer),
            };
            mode = "binary";
            spawn_stream(
                &state,
                waveform_source,
                on_event,
                interval,
                max_items,
                "波形",
                |batch| {
                    InvokeResponseBody::Raw(crate::waveform_binary::encode_waveform_window(&batch))
                },
            );
        }
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
                    json_event(DisplayEvent::RawData),
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
                    json_event(DisplayEvent::RawData),
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
                json_event(DisplayEvent::CanFrames),
            ),
            None => spawn_stream(
                &state,
                CanStreamSource::new(state.can_buffer.clone(), max_items),
                on_event,
                interval,
                max_items,
                "CAN",
                json_event(DisplayEvent::CanFrames),
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
                json_event(DisplayEvent::LogicSamples),
            ),
            None => spawn_stream(
                &state,
                LogicStreamSource::new(state.logic_buffer.clone(), max_items),
                on_event,
                interval,
                max_items,
                "逻辑采样",
                json_event(DisplayEvent::LogicSamples),
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
                json_event(DisplayEvent::DecodedEvents),
            ),
            None => spawn_stream(
                &state,
                DecodedStreamSource::new(state.decoded_buffer.clone(), max_items),
                on_event,
                interval,
                max_items,
                "解码事件",
                json_event(DisplayEvent::DecodedEvents),
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
mod tests;
