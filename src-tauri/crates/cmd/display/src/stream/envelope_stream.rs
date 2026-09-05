//! 波形包络流 — 快照语义 (buffer 版本变化即推送), GPU 加速 + CPU 回退

use std::sync::Arc;
use std::time::Duration;

use app_state::AppState;
use buffer_databuffer::DataBuffer;
use gpu_core::{envelope_minmax_cpu, GpuContext};
use parking_lot::Mutex;
use tauri::ipc::{Channel, InvokeResponseBody};

use super::binary::{encode_envelope, ENVELOPE_WINDOW_CAP};
use super::display_rate;

pub(super) fn spawn_envelope_stream(
    state: &AppState,
    buffer: Arc<Mutex<DataBuffer>>,
    columns: usize,
    on_event: Channel<InvokeResponseBody>,
    interval: Duration,
) {
    let channel_id = on_event.id();
    let mut cancel = subscription::register_cancel(&state.subscriptions, channel_id);
    let subscriptions = state.subscriptions.clone();
    let mut rate = display_rate(
        interval,
        state.data_plane.eval.data_bus.limits().preview_fps_limit,
    );
    tokio::spawn(async move {
        let mut seq: u64 = 0;
        let mut last_version: u64 = 0;
        let mut gpu: Option<std::sync::Arc<GpuContext>> = None;
        loop {
            tokio::select! {
                _ = &mut cancel => break,
                () = tokio::time::sleep(rate.current()) => {
                    let (window, version) = {
                        // block_in_place: 大窗口提取 + 压缩可达数十毫秒, 不占并发 worker
                        let buffer = buffer.clone();
                        tokio::task::block_in_place(move || {
                            let buf = buffer.lock();
                            let version = buf.version();
                            if version == last_version {
                                return (None, version);
                            }
                            let pts = buf.point_count().min(ENVELOPE_WINDOW_CAP);
                            (Some(buf.get_recent(pts)), version)
                        })
                    };
                    let Some(window) = window else {
                        rate.on_idle();
                        continue;
                    };
                    last_version = version;
                    // GPU 上下文惰性获取 (pollster 阻塞, 须在 blocking 区)
                    let envelopes = tokio::task::block_in_place(|| {
                        if gpu.is_none() {
                            gpu = GpuContext::acquire();
                        }
                        let columns = columns.max(1);
                        window.channels.iter().map(|channel| {
                            gpu.as_ref().map_or_else(
                                || envelope_minmax_cpu(channel, columns),
                                |ctx| {
                                    gpu_core::envelope_minmax(ctx, channel, columns)
                                        .unwrap_or_else(|_| {
                                            envelope_minmax_cpu(channel, columns)
                                        })
                                },
                            )
                        })
                        .collect::<Vec<_>>()
                    });
                    seq += 1;
                    let frame = encode_envelope(seq, &window, columns.max(1), &envelopes);
                    if on_event.send(InvokeResponseBody::Raw(frame)).is_err() {
                        break;
                    }
                    rate.on_send();
                }
            }
        }
        subscription::remove_subscription(&subscriptions, channel_id);
        log::debug!("波形包络订阅已结束: {channel_id}");
    });
}
