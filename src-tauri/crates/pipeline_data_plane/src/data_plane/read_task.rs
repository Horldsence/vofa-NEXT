//! Transport 节点读任务 — 每个 open 的 Transport 节点一个
//!
//! 循环: subscribe 收字节 → 合批 (try_recv 排空, 上限取配置快照) → raw 收集 → raw 收集 →
//! 沿全局 BytePlan 路由 (协议解析/帧解码喂入/回注发送) → 统计节流 emit。
//! 广播通道关闭 (连接断开) 时退出并 emit Disconnected。

use std::sync::atomic::Ordering;
use std::time::Instant;
use tauri::AppHandle;
use tokio::sync::broadcast;
use buffer_raw::RawDataDirection;
use vofa_core::{ConnectionState, TransportStats};

use super::{byte_router, frame_dispatch, DataPlaneState, STATS_THROTTLE_MS};

/// Transport 节点读任务
pub(super) async fn read_task(
    app: AppHandle,
    plane: DataPlaneState,
    node_id: String,
    mut rx: broadcast::Receiver<Vec<u8>>,
) {
    use tokio::sync::broadcast::error::{RecvError, TryRecvError};

    log::debug!("数据读任务已启动: {}", node_id);
    let mut dec_cache = crate::decoder_feed::DecoderFeedCache::new();
    let mut last_stats = Instant::now();
    let mut acc_bytes: u64 = 0;
    let mut acc_frames: u64 = 0;
    let mut last_report = Instant::now();

    loop {
        let first = match rx.recv().await {
            Ok(d) => d,
            Err(RecvError::Closed) => break,
            Err(RecvError::Lagged(n)) => {
                plane.metrics.lagged.fetch_add(n, Ordering::Relaxed);
                continue;
            }
        };

        // 自适应合批: try_recv 排空积压并拼接 (协议按字节流解析, 拼接语义安全;
        // 负载越高单批越大, 天然背压自适应)
        let cfg = *plane.pipeline_config.read();
        let mut data = first;
        let mut coalesced = 1usize;
        while coalesced < cfg.coalesce_max_msgs && data.len() < cfg.coalesce_max_bytes_kb * 1024 {
            match rx.try_recv() {
                Ok(mut next) => {
                    data.append(&mut next);
                    coalesced += 1;
                }
                Err(TryRecvError::Lagged(n)) => {
                    plane.metrics.lagged.fetch_add(n, Ordering::Relaxed);
                }
                Err(_) => break,
            }
        }
        plane.metrics.rx_msgs.fetch_add(1, Ordering::Relaxed);
        plane
            .metrics
            .rx_bytes
            .fetch_add(data.len() as u64, Ordering::Relaxed);

        // 按源原始字节收集 (不随解析积压丢失 — 收集在路由之前完成)
        plane.raw_collector_for(&node_id).lock().push_chunk(
            vofa_core::now_us(),
            RawDataDirection::Rx,
            &data,
        );

        // 沿全局 BytePlan 路由 (深度提示取广播积压, 供并行解析判定)
        let t_feed = Instant::now();
        let summary = byte_router::route_bytes(
            &plane,
            Some(&app),
            &node_id,
            &data,
            rx.len(),
            &mut dec_cache,
        )
        .await;
        plane
            .metrics
            .feed_ns
            .fetch_add(t_feed.elapsed().as_nanos() as u64, Ordering::Relaxed);
        plane.metrics.feed_batches.fetch_add(1, Ordering::Relaxed);
        plane
            .metrics
            .eval_ns
            .fetch_add(summary.eval_ns, Ordering::Relaxed);
        plane
            .metrics
            .frames_evaled
            .fetch_add(summary.frames, Ordering::Relaxed);

        // FrameDecoder 被喂入 → 快照评估一次 (decoder 输出来自 last_frame 缓存)
        if summary.decoders_fed {
            frame_dispatch::refresh_snapshot(&plane);
        }

        // 统计 (record_rx 由消费侧上报)
        plane
            .transport
            .lock()
            .await
            .record_rx(&node_id, data.len(), summary.frames);
        acc_bytes += data.len() as u64;
        acc_frames += summary.frames;

        // 统计节流 emit (100ms 窗口)
        if last_stats.elapsed().as_millis() >= STATS_THROTTLE_MS {
            notify_events::emit_transport_rx(
                &app,
                &node_id,
                TransportStats {
                    rx_bytes: acc_bytes,
                    rx_frames: acc_frames,
                    tx_bytes: 0,
                    tx_frames: 0,
                    rx_dropped: 0,
                },
            );
            acc_bytes = 0;
            acc_frames = 0;
            last_stats = Instant::now();
        }

        // 2s 诊断指标
        if last_report.elapsed() >= super::METRICS_REPORT_INTERVAL {
            plane.metrics.report();
            last_report = Instant::now();
        }
    }

    notify_events::emit_transport_state(&app, &node_id, ConnectionState::Disconnected);
    log::debug!("数据读任务已退出: {}", node_id);
}
