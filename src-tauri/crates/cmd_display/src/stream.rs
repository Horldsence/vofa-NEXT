use std::time::Duration;

use app_state::AppState;
use buffer_raw::DirectionFilter;
use pipeline_dispatcher::filtered_sources::{
    FilteredCanStreamSource, FilteredDecodedStreamSource, FilteredLogicStreamSource,
    FilteredRawDataSource,
};
use pipeline_stream::{
    join_or_create_group, leave_group, sharded_stream_loop_map, CanStreamSource,
    DecodedStreamSource, LogicStreamSource, RawDataSource, StreamSource, WaveformSource,
};
use tauri::{ipc::Channel, State};
use vofa_core::Result;

use crate::snapshot::spawn_snapshot;
use crate::{DisplayEvent, DisplayRequest, RawDataOrigin};

fn direction(value: &str) -> DirectionFilter {
    match value.to_ascii_lowercase().as_str() {
        "rx" => DirectionFilter::Rx,
        "tx" => DirectionFilter::Tx,
        _ => DirectionFilter::All,
    }
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

async fn spawn_can_load(
    state: &AppState,
    node_id: &str,
    bitrate_bps: Option<u32>,
    on_event: Channel<DisplayEvent>,
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
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |value| u64::try_from(value.as_micros()).unwrap_or(u64::MAX));
                        value.sample_history(bitrate, now);
                        value.snapshot(bitrate)
                    };
                    if on_event.send(DisplayEvent::CanLoad(snapshot)).is_err() { break; }
                }
            }
        }
        subscription::remove_subscription(&subscriptions, channel_id);
    });
}

#[allow(clippy::too_many_arguments)]
fn spawn_stream<S, F>(
    state: &AppState,
    source: S,
    on_event: Channel<DisplayEvent>,
    group_id: Option<String>,
    interval: Duration,
    min_batch: usize,
    name: &'static str,
    map: F,
) -> Result<String>
where
    S: StreamSource,
    F: Fn(S::Batch) -> DisplayEvent + Send + 'static,
{
    let channel_id = on_event.id();
    let (source, seq, shard_idx, group_key) = join_or_create_group(
        &state.stream_groups,
        group_id,
        channel_id,
        state.pipeline_config.read().max_stream_shards,
        || source,
    )?;
    let cancel = subscription::register_cancel(&state.subscriptions, channel_id);
    let groups = state.stream_groups.clone();
    let subscriptions = state.subscriptions.clone();
    let exit_key = group_key.clone();
    tokio::spawn(async move {
        sharded_stream_loop_map(
            format!("{name}分片{shard_idx}"),
            source,
            on_event,
            shard_idx,
            seq,
            interval,
            min_batch,
            cancel,
            map,
        )
        .await;
        leave_group(&groups, &exit_key);
        subscription::remove_subscription(&subscriptions, channel_id);
    });
    Ok(group_key)
}

/// 订阅任意显示数据。所有请求共享同一命令、事件联合和取消入口。
#[tauri::command]
pub async fn subscribe_display(
    state: State<'_, AppState>,
    request: DisplayRequest,
    on_event: Channel<DisplayEvent>,
    group_id: Option<String>,
    interval_ms: Option<u64>,
    max_items: Option<usize>,
) -> Result<String> {
    let interval = Duration::from_millis(interval_ms.unwrap_or(33));
    let max_items = max_items.unwrap_or(1_000);
    match request {
        snapshot @ (DisplayRequest::GraphOutputs
        | DisplayRequest::CustomInputs
        | DisplayRequest::StringOutputs
        | DisplayRequest::Spectrum) => {
            let id = on_event.id().to_string();
            spawn_snapshot(&state, snapshot, on_event, interval);
            Ok(id)
        }
        DisplayRequest::Waveform { source } => spawn_stream(
            &state,
            WaveformSource::new(state.data_plane.buffer_for(&source)),
            on_event,
            group_id,
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
                    None => return Ok(String::new()),
                },
            };
            if filter_direction.is_empty() && search.trim().is_empty() {
                spawn_stream(
                    &state,
                    RawDataSource::new(collector),
                    on_event,
                    group_id,
                    interval,
                    max_items,
                    "原始数据",
                    DisplayEvent::RawData,
                )
            } else {
                spawn_stream(
                    &state,
                    FilteredRawDataSource::new(
                        collector,
                        direction(&filter_direction),
                        (!search.trim().is_empty()).then_some(search.as_str()),
                    ),
                    on_event,
                    group_id,
                    interval,
                    max_items,
                    "过滤原始数据",
                    DisplayEvent::RawData,
                )
            }
        }
        DisplayRequest::CanFrames { filter } => match filter {
            Some(filter) => spawn_stream(
                &state,
                FilteredCanStreamSource::new(state.can_buffer.clone(), filter),
                on_event,
                group_id,
                interval,
                max_items,
                "过滤 CAN",
                DisplayEvent::CanFrames,
            ),
            None => spawn_stream(
                &state,
                CanStreamSource::new(state.can_buffer.clone(), max_items),
                on_event,
                group_id,
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
                group_id,
                interval,
                max_items,
                "过滤逻辑采样",
                DisplayEvent::LogicSamples,
            ),
            None => spawn_stream(
                &state,
                LogicStreamSource::new(state.logic_buffer.clone(), max_items),
                on_event,
                group_id,
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
                group_id,
                interval,
                max_items,
                "过滤解码事件",
                DisplayEvent::DecodedEvents,
            ),
            None => spawn_stream(
                &state,
                DecodedStreamSource::new(state.decoded_buffer.clone(), max_items),
                on_event,
                group_id,
                interval,
                max_items,
                "解码事件",
                DisplayEvent::DecodedEvents,
            ),
        },
        DisplayRequest::CanLoad {
            node_id,
            bitrate_bps,
        } => {
            let id = on_event.id().to_string();
            spawn_can_load(&state, &node_id, bitrate_bps, on_event, interval).await;
            Ok(id)
        }
    }
}

/// 取消任意显示订阅。
#[tauri::command]
pub async fn unsubscribe_display(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    subscription::cancel_subscription(&state.subscriptions, channel_id);
    Ok(())
}
