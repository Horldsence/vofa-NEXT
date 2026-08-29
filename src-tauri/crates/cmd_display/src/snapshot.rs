use std::time::Duration;

use app_state::AppState;
use tauri::ipc::Channel;

use crate::{DisplayEvent, DisplayRequest};

/// 启动 latest-value 快照订阅。快照按版本或值变化推送，不建立分片组。
pub fn spawn_snapshot(
    state: &AppState,
    request: DisplayRequest,
    on_event: Channel<DisplayEvent>,
    interval: Duration,
) {
    let eval = state.eval_state();
    let channel_id = on_event.id();
    let mut cancel = subscription::register_cancel(&state.subscriptions, channel_id);
    let subscriptions = state.subscriptions.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        let mut last_tick = None;
        let mut last_custom = None;
        loop {
            tokio::select! {
                _ = &mut cancel => break,
                _ = ticker.tick() => {
                    let event = match &request {
                        DisplayRequest::GraphOutputs => {
                            let value = eval.output_snapshot.lock().clone();
                            if last_tick == Some(value.tick) { continue; }
                            last_tick = Some(value.tick);
                            DisplayEvent::GraphOutputs(value)
                        }
                        DisplayRequest::StringOutputs => {
                            let value = eval.text_output_snapshot.lock().clone();
                            if last_tick == Some(value.tick) { continue; }
                            last_tick = Some(value.tick);
                            DisplayEvent::StringOutputs(value)
                        }
                        DisplayRequest::Spectrum => {
                            let value = eval.spectrum_snapshot.lock().clone();
                            if value.is_empty() { continue; }
                            DisplayEvent::Spectrum(value)
                        }
                        DisplayRequest::CustomInputs => {
                            let outputs = eval.output_snapshot.lock();
                            let graphs = eval.graphs.lock();
                            let mut inputs = std::collections::HashMap::new();
                            for graph in graphs.values() {
                                inputs.extend(graph.collect_custom_inputs(&outputs.values));
                            }
                            if last_custom.as_ref() == Some(&inputs) { continue; }
                            last_custom = Some(inputs.clone());
                            DisplayEvent::CustomInputs(pipeline_data_plane::CustomInputBatch { inputs })
                        }
                        _ => unreachable!("stream request routed to snapshot task"),
                    };
                    if on_event.send(event).is_err() { break; }
                }
            }
        }
        subscription::remove_subscription(&subscriptions, channel_id);
    });
}
