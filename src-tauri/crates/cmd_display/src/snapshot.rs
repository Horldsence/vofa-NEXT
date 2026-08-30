use std::time::Duration;

use app_state::AppState;
use pipeline_data_plane::{CustomInputBatch, GraphEvalState};
use tauri::ipc::{Channel, InvokeResponseBody};

use crate::{DisplayEvent, DisplayRequest};

/// 收集 Custom 节点输入快照。
///
/// 数据平面求值时的锁序是 `graphs -> output_snapshot`。这里不能反向同时持有
/// `output_snapshot -> graphs`，否则活跃端口一开始产帧就可能与显示订阅互锁，
/// 继而让 graph_outputs 永远停留在连接前的默认值。先复制 latest-value 输出并
/// 释放锁，再读取图；图恰好在两步之间更新时，下一次订阅 tick 会自然收敛。
fn collect_custom_inputs(eval: &GraphEvalState) -> CustomInputBatch {
    let outputs = eval.output_snapshot.lock().values.clone();
    let graphs = eval.graphs.lock();
    let mut inputs = std::collections::HashMap::new();
    for graph in graphs.values() {
        inputs.extend(graph.collect_custom_inputs(&outputs));
    }
    CustomInputBatch { inputs }
}

/// 启动 latest-value 快照订阅。快照按版本或值变化推送，不建立分片组。
pub fn spawn_snapshot(
    state: &AppState,
    request: DisplayRequest,
    on_event: Channel<InvokeResponseBody>,
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
                            let value = collect_custom_inputs(&eval);
                            if last_custom.as_ref() == Some(&value.inputs) { continue; }
                            last_custom = Some(value.inputs.clone());
                            DisplayEvent::CustomInputs(value)
                        }
                        _ => unreachable!("stream request routed to snapshot task"),
                    };
                    let body = match serde_json::to_string(&event) {
                        Ok(json) => InvokeResponseBody::Json(json),
                        Err(error) => {
                            log::error!("显示快照序列化失败: {error}");
                            break;
                        }
                    };
                    if on_event.send(body).is_err() { break; }
                }
            }
        }
        subscription::remove_subscription(&subscriptions, channel_id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_inputs_collection_keeps_latest_value_contract() {
        let state = AppState::new();
        state.data_plane.eval.output_snapshot.lock().values.insert(
            "source".into(),
            std::iter::once(("out".into(), 42.0)).collect(),
        );

        let batch = collect_custom_inputs(&state.data_plane.eval);

        assert!(batch.inputs.is_empty(), "没有 Custom 节点时不应伪造输入");
        assert!(
            state.data_plane.eval.output_snapshot.try_lock().is_some(),
            "采集结束后不得遗留 output_snapshot 锁"
        );
        assert!(
            state.data_plane.eval.graphs.try_lock().is_some(),
            "采集结束后不得遗留 graphs 锁"
        );
    }
}
