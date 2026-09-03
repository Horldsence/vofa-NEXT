//! Transport 节点读任务 — 每个 open 的 Transport 节点一个
//!
//! 循环: subscribe 收字节 → 合批 (try_recv 排空, 上限取配置快照) → raw 收集 → raw 收集 →
//! 沿全局 BytePlan 路由 (协议解析/帧解码喂入/回注发送) → 统计节流 emit。
//! 广播通道关闭 (连接断开) 时退出并 emit Disconnected。

use buffer_raw::RawDataDirection;
use data_bus::{AdaptiveController, RuntimeLimits};
use kind::NodeKind;
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::Ordering;
use std::time::Instant;
use tauri::AppHandle;
use tokio::sync::broadcast;
use vofa_core::{ConnectionState, TransportStats};

use super::{byte_router, eval_worker, frame_dispatch, DataPlaneState, STATS_THROTTLE_MS};
use crate::feed_parallel::FEED_PARALLEL_UNIT;

pub(super) fn mark_downstream_disconnected(plane: &DataPlaneState, transport_id: &str) {
    let plan = plane.byte_plan.lock();
    let nodes = plane.global_nodes.lock();
    let mut pending = VecDeque::from([transport_id.to_string()]);
    let mut visited = HashSet::new();
    while let Some(source) = pending.pop_front() {
        if !visited.insert(source.clone()) {
            continue;
        }
        for route in plan.routes_for(&source) {
            if matches!(
                nodes.get(&route.target).map(|node| &node.kind),
                Some(NodeKind::Protocol { .. })
            ) {
                plane
                    .eval
                    .data_bus
                    .set_source_status(&route.target, data_bus::SampleStatus::Disconnected);
            }
            pending.push_back(route.target.clone());
        }
    }
}

/// 合批的样本时长上限 (秒) — 单批代表的采样时长不超过 50ms,
/// 防止高负载下"越合越大"的正反馈把处理延迟滚成秒级 (批一旦超过
/// 评估吞吐的 1/8 就开始丢批, 批越大丢得越多)
const MAX_BATCH_SAMPLE_SECONDS: f64 = 0.05;

/// 传输名义字节速率 (C2: 合批样本时长上限的换算基准)
/// TestData = 帧率 × 每帧字节数; 串口 = 波特率线速 (字节/s)
fn nominal_bytes_per_sec(
    live: Option<&transport_core::LiveNodeHandle>,
    node_id: &str,
    avg_bytes_per_frame: f64,
) -> Option<f64> {
    let config = live?.config_of(node_id)?;
    match config {
        vofa_core::TransportConfig::TestData(c) => {
            Some(f64::from(c.sample_rate) * avg_bytes_per_frame.max(1.0))
        }
        vofa_core::TransportConfig::Serial(c) => {
            let parity_bits = u32::from(c.parity != vofa_core::Parity::None);
            let stop_bits = match c.stop_bits {
                vofa_core::StopBits::One => 1,
                vofa_core::StopBits::Two => 2,
            };
            let bits_per_byte = 1 + u32::from(c.data_bits) + parity_bits + stop_bits;
            Some(f64::from(c.baud_rate) / f64::from(bits_per_byte))
        }
        _ => None,
    }
}

/// Transport 节点读任务
pub(super) async fn read_task(
    app: AppHandle,
    plane: DataPlaneState,
    node_id: String,
    mut rx: broadcast::Receiver<Vec<u8>>,
) {
    use tokio::sync::broadcast::error::{RecvError, TryRecvError};

    log::debug!("数据读任务已启动: {node_id}");
    let mut dec_cache = crate::decoder_feed::DecoderFeedCache::new();
    let mut last_stats = Instant::now();
    let mut acc_bytes: u64 = 0;
    let mut acc_frames: u64 = 0;
    let mut last_report = Instant::now();
    let mut controller = AdaptiveController::default();
    // 每帧平均字节数 (EMA) — 合批样本时长上限的折算输入
    let mut avg_bytes_per_frame = 16.0_f64;
    // 启动时取一次轻量句柄: 每批的 TestData 开关查询 / 运行态配置 / rx 统计上报
    // 都免 manager 全局锁 (逐批锁会与 open/close/其他传输串行化)
    let live = plane.transport.lock().await.live_handle(&node_id);
    // TestData 生成停止边沿检测 (None = 非 TestData 节点, 永不触发)
    let mut was_generating = live
        .as_ref()
        .and_then(transport_core::LiveNodeHandle::test_data_running_state)
        .unwrap_or(false);

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
        // 负载越高单批越大, 天然背压自适应)。上限取字节目标与"样本时长 50ms"
        // 折算字节数的较小者 — 高速率下单批不失控 (C2)
        let coalesce_cap = {
            let bytes_target = controller.target_batch_bytes();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // 速率非负, ceil 后截断无害
            let nominal_bound = nominal_bytes_per_sec(live.as_ref(), &node_id, avg_bytes_per_frame)
                .map_or(usize::MAX, |bytes_per_sec| {
                    let sample_bound = (bytes_per_sec * MAX_BATCH_SAMPLE_SECONDS).ceil() as usize;
                    let floor = avg_bytes_per_frame.max(1.0) as usize;
                    sample_bound.max(floor)
                });
            bytes_target.min(nominal_bound)
        };
        let mut data = first;
        let mut coalesced = 1usize;
        while coalesced < 1024 && data.len() < coalesce_cap {
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
        plane.metrics.rx_msgs.fetch_add(
            u64::try_from(coalesced).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        plane
            .metrics
            .rx_bytes
            .fetch_add(data.len() as u64, Ordering::Relaxed);

        // TestData 停止生成边沿: 排空广播积压 (已解析的当前批次正常处理,
        // 其后的排队消息全部丢弃), 波形立即冻结在停止时刻, 不再拖尾滚动
        let generating = live
            .as_ref()
            .and_then(transport_core::LiveNodeHandle::test_data_running_state);
        if was_generating && generating == Some(false) {
            let mut dropped = 0_u64;
            loop {
                match rx.try_recv() {
                    Ok(_) => dropped += 1,
                    Err(TryRecvError::Lagged(n)) => {
                        plane.metrics.lagged.fetch_add(n, Ordering::Relaxed);
                    }
                    Err(_) => break,
                }
            }
            // 已解析未评估的队列批同样清空: 波形冻结在停止时刻, 不拖尾
            dropped += eval_worker::clear_downstream_queues(&plane, &node_id);
            if dropped > 0 {
                log::debug!("测试数据停止生成, 丢弃排队积压 {dropped} 条: {node_id}");
            }
        }
        was_generating = generating.unwrap_or(false);

        // 按源原始字节收集 (不随解析积压丢失 — 收集在路由之前完成)
        plane.raw_collector_for(&node_id).lock().push_chunk(
            vofa_core::now_us(),
            RawDataDirection::Rx,
            &data,
        );

        // 沿全局 BytePlan 路由 (深度提示取广播积压, 供并行解析判定)
        let t_feed = Instant::now();
        let depth_hint = rx.len().max(
            controller
                .workers()
                .saturating_sub(1)
                .saturating_mul(FEED_PARALLEL_UNIT),
        );
        let summary = byte_router::route_bytes(
            &plane,
            Some(&app),
            &node_id,
            &data,
            depth_hint,
            &mut dec_cache,
            live.as_ref(),
        )
        .await;
        let service_time = t_feed.elapsed();
        let cfg = *plane.pipeline_config.read();
        let queued = rx.len();
        // queue_fill 用广播通道真实容量 (通道均以 INGEST_CHANNEL_CAPACITY 创建),
        // 不再硬编码 256
        let queue_fill = f64::from(u32::try_from(queued).unwrap_or(u32::MAX))
            / f64::from(u32::try_from(vofa_core::INGEST_CHANNEL_CAPACITY).unwrap_or(u32::MAX));
        let queue_age =
            service_time.saturating_mul(u32::try_from(queued.min(1_024)).unwrap_or(1_024));
        controller.observe(
            queue_fill,
            queue_age,
            service_time,
            data.len(),
            RuntimeLimits {
                max_workers: cfg.max_workers,
                memory_budget_mb: cfg.memory_budget_mb,
                preview_fps_limit: cfg.preview_fps_limit,
                preview_bandwidth_mb_per_sec: cfg.preview_bandwidth_mb_per_sec,
            },
        );
        plane.metrics.feed_ns.fetch_add(
            u64::try_from(t_feed.elapsed().as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        plane.metrics.feed_batches.fetch_add(1, Ordering::Relaxed);
        // eval 耗时/帧数由 eval worker 计量 (摄入/评估解耦)

        // FrameDecoder 被喂入 → 快照评估一次 (decoder 输出来自 last_frame 缓存)
        if summary.decoders_fed {
            frame_dispatch::refresh_snapshot(&plane);
        }

        // 统计 (record_rx 由消费侧上报, 经轻量句柄免全局锁)
        if let Some(live) = &live {
            live.record_rx(data.len(), summary.frames);
        }
        if summary.frames > 0 {
            #[allow(clippy::cast_precision_loss)] // EMA 统计近似, 精度损失无影响
            let batch_avg = data.len() as f64 / summary.frames as f64;
            avg_bytes_per_frame = avg_bytes_per_frame.mul_add(0.9, batch_avg * 0.1);
        }
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

        // 2s 诊断指标 (含缓冲降载增量汇总)
        if last_report.elapsed() >= super::METRICS_REPORT_INTERVAL {
            plane.report_buffer_overflow_delta();
            plane.metrics.report();
            last_report = Instant::now();
        }
    }

    mark_downstream_disconnected(&plane, &node_id);
    notify_events::emit_transport_state(&app, &node_id, ConnectionState::Disconnected);
    log::debug!("数据读任务已退出: {node_id}");
}
