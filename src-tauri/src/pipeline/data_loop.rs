use crate::state::GraphEvalState;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use parking_lot::Mutex;
use std::sync::Arc;
use vofa_next_buffer::{DataBuffer, RawDataCollector};
use vofa_next_core::{CanBuffer, CanLoadStats, CanFrameBatch, ConnectionState, DataFrame, DecodedBuffer, DecodedEventBatch, LogicBuffer, LogicSampleBatch, TransportStats};
use vofa_next_protocol::ProtocolEngine;

const STATS_THROTTLE_MS: u128 = 100;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    app: AppHandle,
    mut rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
    protocol: Arc<Mutex<Box<dyn ProtocolEngine>>>,
    buffer: Arc<Mutex<DataBuffer>>,
    eval_state: GraphEvalState,
    raw_data_collector: Arc<Mutex<RawDataCollector>>,
    can_buffer: Arc<Mutex<CanBuffer>>,
    can_load_stats: Arc<Mutex<CanLoadStats>>,
    logic_buffer: Arc<Mutex<LogicBuffer>>,
    decoded_buffer: Arc<Mutex<DecodedBuffer>>,
) {
    log::info!("数据循环已启动");

    // 解析 task 用的 mpsc (大容量缓冲, 避免反压到 data_loop)
    let (parse_tx, mut parse_rx) = mpsc::channel::<Vec<u8>>(2048);
    let app2 = app.clone();
    let proto2 = protocol.clone();
    let buf2 = buffer.clone();
    let eval2 = eval_state;
    let raw_collector2 = raw_data_collector;
    let can_buffer2 = can_buffer;
    let can_load_stats2 = can_load_stats;
    let logic_buffer2 = logic_buffer;
    let decoded_buffer2 = decoded_buffer;

    let parse_task = tokio::spawn(async move {
        let mut detection_notified = false;
        let mut frame_batch: Vec<DataFrame> = Vec::with_capacity(128);
        let mut last_stats = Instant::now();
        let mut acc_bytes: u64 = 0;
        let mut acc_frames: u64 = 0;

        while let Some(data) = parse_rx.recv().await {
            // 1. 收集原始数据 (通过 Channel 周期性推送, 替代每包 emit)
            raw_collector2.lock().push_chunk(now_us(), &data);

            // 2. 协议解析
            let frames = proto2.lock().feed(&data);
            acc_bytes += data.len() as u64;
            acc_frames += frames.len() as u64;

            // 2.x CAN 帧解析 (slcan/candleLight) — 非 CAN 协议返回空 Vec
            let can_frames = proto2.lock().feed_can(&data);
            if !can_frames.is_empty() {
                // push 到 can_buffer
                {
                    let mut buf = can_buffer2.lock();
                    for f in &can_frames {
                        buf.push(f.clone());
                    }
                }
                // push 到负载统计器 (仅统计 Rx 方向, 避免发送帧重复计入)
                {
                    let mut stats = can_load_stats2.lock();
                    for f in &can_frames {
                        if f.direction == vofa_next_core::CanDirection::Rx {
                            stats.push(f);
                        }
                    }
                }
                // emit 批次事件 (实时推送到前端, 供监听 transport:can-frames 的组件使用)
                let _ = app2.emit(
                    "transport:can-frames",
                    &CanFrameBatch { frames: can_frames.clone() },
                );
            }

            // 2.x 逻辑采样 + 解码事件 (LogicDecoder 协议)
            let logic_samples = proto2.lock().feed_logic(&data);
            if !logic_samples.is_empty() {
                {
                    let mut lb = logic_buffer2.lock();
                    for s in &logic_samples {
                        lb.push(s.clone());
                    }
                }
                let _ = app2.emit(
                    "transport:logic-samples",
                    &LogicSampleBatch { samples: logic_samples },
                );
            }
            let decoded_events = proto2.lock().feed_decoded(&data);
            if !decoded_events.is_empty() {
                {
                    let mut db = decoded_buffer2.lock();
                    for e in &decoded_events {
                        db.push(e.clone());
                    }
                }
                let _ = app2.emit(
                    "transport:decoded-events",
                    &DecodedEventBatch { events: decoded_events },
                );
            }

            // 2.1 自动通道检测通知 (一次性)
            if !detection_notified {
                let p = proto2.lock();
                if p.is_auto_mode() {
                    if let Some(n) = p.detected_channels() {
                        crate::notify::channels_detected(&app2, n);
                        detection_notified = true;
                    }
                }
            }

            // 2.2 帧解码器: 喂入原始字节, 更新 decoder_states.last_frame
            //     必须在 evaluate 之前完成, evaluate 阶段从 last_frame 读取输出
            //     返回 has_decoders: 是否存在 FrameDecoder 节点 (供 frames 空时决策)
            let has_decoders = super::decoder_feed::feed_frame_decoders(&eval2, &data, now_us());

            // 3. 推入缓冲区 + 评估节点图 + 收集派生值 (每帧实时计算)
            //    三步必须在同一帧内顺序执行, 保证 derived 与 timestamps 对齐
            if !frames.is_empty() {
                for f in &frames {
                    // 3.1 push 原始帧到 buffer
                    {
                        let mut buf = buf2.lock();
                        buf.push_frame(f);
                    }
                    // 3.2 评估所有 tab 的图, 更新 output_snapshot
                    //     (ticker task 会按 60 FPS 推送到前端)
                    super::graph_eval::evaluate_all_graphs_with(&eval2, f);
                    // 3.3 从 snapshot 收集派生值, push 到 buffer.derived_buffers
                    //     与本帧 push_frame 的时间戳对齐
                    {
                        let mut buf = buf2.lock();
                        super::graph_eval::push_derived_from_snapshot(&eval2, &mut buf);
                    }
                }
                frame_batch.extend(frames);
            } else if has_decoders {
                // RawData 等协议下 frames 为空, 但 FrameDecoder 节点存在时
                // 仍需 evaluate 一次以更新 output_snapshot (decoder 输出来自 last_frame 缓存)
                super::graph_eval::evaluate_all_graphs_with(&eval2, &DataFrame::new(vec![]));
                // 不 push_derived (无 push_frame, 时间戳未对齐)
            }

            // 4. 批量 emit 帧 (一次 IPC 替代 N 次)
            if !frame_batch.is_empty() {
                let _ = app2.emit("transport:frames", &frame_batch);
                frame_batch.clear();
            }

            // 5. 统计节流 emit
            let now = Instant::now();
            if now.duration_since(last_stats).as_millis() >= STATS_THROTTLE_MS {
                let _ = app2.emit(
                    "transport:rx",
                    &TransportStats {
                        rx_bytes: acc_bytes,
                        rx_frames: acc_frames,
                        tx_bytes: 0,
                        tx_frames: 0,
                    },
                );
                acc_bytes = 0;
                acc_frames = 0;
                last_stats = now;
            }
        }

        // mpsc 关闭 → 传输已断开
        let _ = app2.emit("transport:state", ConnectionState::Disconnected);
        log::info!("解析任务已退出");
    });

    // data_loop: 快速消费 broadcast, 转发到 mpsc (不阻塞在 emit/解析上)
    loop {
        match rx.recv().await {
            Ok(data) => {
                if parse_tx.send(data).await.is_err() {
                    log::info!("解析任务已退出, 停止数据循环");
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                log::info!("数据广播通道已关闭");
                break;
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                log::warn!("数据广播落后 {} 条", n);
            }
        }
    }

    // 关闭 mpsc, 等待解析 task 刷完剩余数据
    drop(parse_tx);
    let _ = parse_task.await;
    log::info!("数据循环已退出");
}

fn now_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}
