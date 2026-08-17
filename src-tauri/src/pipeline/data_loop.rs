use crate::state::GraphEvalState;
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use vofa_next_buffer::{DataBuffer, RawDataCollector, RawDataDirection};
use vofa_next_core::{
    CanBuffer, CanLoadStats, ConnectionState, DataFrame, DecodedBuffer, LogicBuffer,
    PipelineConfig, TransportStats,
};
use vofa_next_protocol::ProtocolEngine;

const STATS_THROTTLE_MS: u128 = 100;
/// 诊断指标输出间隔
const METRICS_REPORT_INTERVAL: Duration = Duration::from_secs(2);

/// 流水线诊断指标 — data_loop / feed_task / eval_task 共享, 每 2s 输出一次。
/// 用于定位高通量 (7MB/s+) 下的瓶颈段: raw 收集 / 各级反压 / 协议解析 / 图评估。
#[derive(Default)]
struct PipelineMetrics {
    /// data_loop 收到的消息数 / 字节数
    rx_msgs: AtomicU64,
    rx_bytes: AtomicU64,
    /// raw 收集 (push_chunk) 累计耗时 ns — 含 collector 锁竞争
    push_chunk_ns: AtomicU64,
    /// data_loop → feed_task mpsc.send 累计等待 ns — 非 0 说明 feed 消费跟不上
    send_wait_ns: AtomicU64,
    /// feed_task: 处理批次数 / 合批前消息数 / 字节数
    parse_batches: AtomicU64,
    parse_msgs: AtomicU64,
    parse_bytes: AtomicU64,
    /// feed_task: 协议 feed (单次全量解析 + 帧解码器喂入) 累计耗时 ns
    feed_ns: AtomicU64,
    /// feed 段细分: split_aligned (含锁) / 解析 (顺序=p.feed 本身; 并行=spawn+join) /
    /// 帧解码器喂入 / 本窗口解析出的 DataFrame 总数
    split_ns: AtomicU64,
    parse_join_ns: AtomicU64,
    decoder_feed_ns: AtomicU64,
    frames_parsed: AtomicU64,
    /// feed_task → eval_task 递交累计等待 ns — 非 0 说明 eval 消费跟不上
    eval_wait_ns: AtomicU64,
    /// eval_task: 批次数 / 累计耗时 ns (push_frame + 图评估 + 派生)
    eval_batches: AtomicU64,
    eval_ns: AtomicU64,
    /// eval 段细分: push_frame / 图评估 / 派生收集 / spectrum / 本窗口评估的帧总数
    eval_push_frame_ns: AtomicU64,
    eval_graph_ns: AtomicU64,
    eval_derived_ns: AtomicU64,
    eval_spectrum_ns: AtomicU64,
    frames_evaled: AtomicU64,
    /// 并行解析批次数 / 窗口内 worker 数峰值
    parallel_batches: AtomicU64,
    max_workers_seen: AtomicU64,
    /// broadcast Lagged 丢弃的消息数 (累计到 100ms transport:rx 统计窗口外露)
    dropped_msgs: AtomicU64,
    /// broadcast Lagged 丢弃的消息数 (2s 诊断窗口)
    lagged_msgs: AtomicU64,
}

impl PipelineMetrics {
    /// 输出并重置一个报告窗口的指标; 窗口内无活动则不输出。
    /// 窗口内有 Lagged 丢弃时提级为 warn (指标本身就是异常信号, 且避免被 warn 刷屏淹没)
    fn report(&self, mpsc_depth: usize, mpsc_cap: usize) {
        let rx_msgs = self.rx_msgs.swap(0, Ordering::Relaxed);
        let lagged = self.lagged_msgs.swap(0, Ordering::Relaxed);
        if rx_msgs == 0 && lagged == 0 {
            return;
        }
        let rx_bytes = self.rx_bytes.swap(0, Ordering::Relaxed);
        let push_ns = self.push_chunk_ns.swap(0, Ordering::Relaxed);
        let send_ns = self.send_wait_ns.swap(0, Ordering::Relaxed);
        let batches = self.parse_batches.swap(0, Ordering::Relaxed);
        let parse_msgs = self.parse_msgs.swap(0, Ordering::Relaxed);
        let parse_bytes = self.parse_bytes.swap(0, Ordering::Relaxed);
        let feed_ns = self.feed_ns.swap(0, Ordering::Relaxed);
        let eval_wait_ns = self.eval_wait_ns.swap(0, Ordering::Relaxed);
        let eval_batches = self.eval_batches.swap(0, Ordering::Relaxed);
        let eval_ns = self.eval_ns.swap(0, Ordering::Relaxed);
        let split_ns = self.split_ns.swap(0, Ordering::Relaxed);
        let parse_join_ns = self.parse_join_ns.swap(0, Ordering::Relaxed);
        let decoder_feed_ns = self.decoder_feed_ns.swap(0, Ordering::Relaxed);
        let frames_parsed = self.frames_parsed.swap(0, Ordering::Relaxed);
        let eval_push_ns = self.eval_push_frame_ns.swap(0, Ordering::Relaxed);
        let eval_graph_ns = self.eval_graph_ns.swap(0, Ordering::Relaxed);
        let eval_derived_ns = self.eval_derived_ns.swap(0, Ordering::Relaxed);
        let eval_spectrum_ns = self.eval_spectrum_ns.swap(0, Ordering::Relaxed);
        let frames_evaled = self.frames_evaled.swap(0, Ordering::Relaxed);
        let par_batches = self.parallel_batches.swap(0, Ordering::Relaxed);
        let par_peak = self.max_workers_seen.swap(0, Ordering::Relaxed);
        let secs = METRICS_REPORT_INTERVAL.as_secs_f64();
        let mut msg = format!(
            "流水线指标: rx {:.1}MB/s ({} 消息/s), mpsc 深度 {}/{} | \
             push_chunk 均 {}µs, send等待 均 {}µs | \
             feed {} 批 (合批均 {:.1} 消息, {:.1}MB/s), feed 均 {:.2}ms \
             [split 均 {:.2}ms, 解析 均 {:.2}ms, decoder 均 {:.2}ms], 帧均 {}/批, \
             eval递交等待 均 {}µs | \
             eval {} 批, eval 均 {:.2}ms/批 \
             [push_frame 均 {:.2}ms, 图评估 均 {:.2}ms, 派生 均 {:.2}ms, 频谱 均 {:.2}ms], \
             帧均 {}/批 | Lagged 丢弃 {} 条",
            rx_bytes as f64 / secs / 1e6,
            (rx_msgs as f64 / secs) as u64,
            mpsc_depth,
            mpsc_cap,
            push_ns.checked_div(rx_msgs).unwrap_or(0) / 1000,
            send_ns.checked_div(rx_msgs).unwrap_or(0) / 1000,
            batches,
            parse_msgs as f64 / batches.max(1) as f64,
            parse_bytes as f64 / secs / 1e6,
            feed_ns as f64 / batches.max(1) as f64 / 1e6,
            split_ns as f64 / batches.max(1) as f64 / 1e6,
            parse_join_ns as f64 / batches.max(1) as f64 / 1e6,
            decoder_feed_ns as f64 / batches.max(1) as f64 / 1e6,
            frames_parsed / batches.max(1),
            eval_wait_ns.checked_div(batches).unwrap_or(0) / 1000,
            eval_batches,
            eval_ns as f64 / eval_batches.max(1) as f64 / 1e6,
            eval_push_ns as f64 / eval_batches.max(1) as f64 / 1e6,
            eval_graph_ns as f64 / eval_batches.max(1) as f64 / 1e6,
            eval_derived_ns as f64 / eval_batches.max(1) as f64 / 1e6,
            eval_spectrum_ns as f64 / eval_batches.max(1) as f64 / 1e6,
            frames_evaled / eval_batches.max(1),
            lagged,
        );
        // 有并行批次时附加并行解析字段 (无并行批次保持现状输出格式)
        if par_batches > 0 {
            msg.push_str(&format!(" | 并行 {} 批 (峰值 {} worker)", par_batches, par_peak));
        }
        if lagged > 0 {
            log::warn!("{}", msg);
        } else {
            log::debug!("{}", msg);
        }
    }
}

/// feed_task → eval_task 的帧批次 (两段流水线的连接消息)
struct EvalBatch {
    frames: Vec<DataFrame>,
    /// frames 为空但存在 FrameDecoder 节点时, 仍需空帧评估一次 (decoder 输出来自 last_frame 缓存)
    force_eval: bool,
}

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
    config: Arc<RwLock<PipelineConfig>>,
) {
    log::debug!("数据循环已启动");

    // 解析 task 用的 mpsc (容量取自建通道时的配置快照, 传输层已是 64KB 大块,
    // 默认 256 条 ≈ 16MB 缓冲)
    let parse_cap = config.read().parse_channel_cap;
    let (parse_tx, mut parse_rx) = mpsc::channel::<Vec<u8>>(parse_cap);
    let metrics = Arc::new(PipelineMetrics::default());
    let app2 = app.clone();
    let proto2 = protocol.clone();
    let can_buffer2 = can_buffer;
    let can_load_stats2 = can_load_stats;
    let logic_buffer2 = logic_buffer;
    let decoded_buffer2 = decoded_buffer;

    // 两段流水线 (负载均衡到不同核):
    //   feed_task: 合批 + 协议解析 + CAN/逻辑/解码缓冲 + 帧解码器喂入 (字节流段)
    //   eval_task: push_frame + 图评估 + 派生收集 (计算密集段)
    let (eval_tx, mut eval_rx) = mpsc::channel::<EvalBatch>(256);

    let metrics_eval = metrics.clone();
    let app3 = app.clone();
    let buf2 = buffer.clone();
    let eval2 = eval_state.clone();
    let eval_task = tokio::spawn(async move {
        while let Some(batch) = eval_rx.recv().await {
            let t = Instant::now();
            if !batch.frames.is_empty() {
                let mut buf = buf2.lock();
                // 细分计时 (观测用, 不影响行为): push_frame / 图评估 / 派生 / 频谱
                let mut breakdown = super::graph_eval::EvalBreakdown::default();
                super::graph_eval::process_frames_batch(
                    &eval2,
                    &batch.frames,
                    &mut buf,
                    &mut breakdown,
                );
                metrics_eval
                    .eval_push_frame_ns
                    .fetch_add(breakdown.push_frame_ns, Ordering::Relaxed);
                metrics_eval
                    .eval_graph_ns
                    .fetch_add(breakdown.graph_eval_ns, Ordering::Relaxed);
                metrics_eval
                    .eval_derived_ns
                    .fetch_add(breakdown.derived_ns, Ordering::Relaxed);
                metrics_eval
                    .eval_spectrum_ns
                    .fetch_add(breakdown.spectrum_ns, Ordering::Relaxed);
                metrics_eval
                    .frames_evaled
                    .fetch_add(batch.frames.len() as u64, Ordering::Relaxed);
            } else if batch.force_eval {
                // RawData 等协议下 frames 为空, 但 FrameDecoder 节点存在时
                // 仍需 evaluate 一次以更新 output_snapshot (decoder 输出来自 last_frame 缓存)
                super::graph_eval::evaluate_all_graphs_with(&eval2, &DataFrame::new(vec![]));
            }
            metrics_eval.eval_batches.fetch_add(1, Ordering::Relaxed);
            metrics_eval
                .eval_ns
                .fetch_add(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
        }
        // eval 通道关闭 → feed_task 已退出 → 传输已断开
        let _ = app3.emit("transport:state", ConnectionState::Disconnected);
        log::debug!("评估任务已退出");
    });

    let metrics_parse = metrics.clone();
    let cfg_feed = config.clone();
    let eval_feed = eval_state;
    let feed_task = tokio::spawn(async move {
        let mut detection_notified = false;
        let mut last_stats = Instant::now();
        let mut acc_bytes: u64 = 0;
        let mut acc_frames: u64 = 0;
        // 并行解析状态: worker 池 + 模式标记 + 协议支持探测 (None = 未探测)
        let mut parallel = super::feed_parallel::ParallelFeeder::new();
        let mut in_parallel = false;
        let mut parallel_supported: Option<bool> = None;
        let mut last_workers = 1usize;
        // FrameDecoder 配置缓存 (按 graphs_version 失效, 避免每批抢 graphs 锁)
        let mut dec_cache = super::decoder_feed::DecoderFeedCache::new();

        while let Some(mut data) = parse_rx.recv().await {
            // 1. 自适应合批: 用 try_recv 排空积压并拼接成大 buffer (上限取配置快照,
            //    默认 64 条 / 256 KiB)。协议按字节流解析, 拼接语义安全; 负载越高单批
            //    越大, 消息处理次数随之下降, 天然背压自适应。原始字节收集已在
            //    data_loop 完成, 不在此排队。
            let cfg = *cfg_feed.read();
            let mut coalesced = 1usize;
            while coalesced < cfg.coalesce_max_msgs && data.len() < cfg.coalesce_max_bytes_kb * 1024 {
                match parse_rx.try_recv() {
                    Ok(mut next) => {
                        data.append(&mut next);
                        coalesced += 1;
                    }
                    Err(_) => break,
                }
            }
            metrics_parse
                .parse_msgs
                .fetch_add(coalesced as u64, Ordering::Relaxed);
            metrics_parse
                .parse_bytes
                .fetch_add(data.len() as u64, Ordering::Relaxed);

            // 1.5 自动并行分流 (对齐 send 侧 stream.rs 语义): 积压低时 workers=1
            //     走顺序路径 (零变化); 积压高时按帧边界切开并行解析。
            //     仅帧定界协议支持 (split_aligned 返回 Some), LogicDecoder / RawData 回退顺序
            let depth = parse_rx.len();
            let workers = super::feed_parallel::workers_needed(depth, data.len(), &cfg);
            let can_parallel = workers > 1
                && *parallel_supported.get_or_insert_with(|| {
                    // 空数据探测: 支持的协议返回 Some (可能为空 Vec), 不支持返回 None
                    proto2.lock().split_aligned(&[], 2).is_some()
                });
            let eff_workers = if can_parallel { workers } else { 1 };
            if eff_workers != last_workers {
                log::debug!(
                    "并行解析 worker 数 {} → {} (积压 {}/{}, 批次 {}KB)",
                    last_workers,
                    eff_workers,
                    depth,
                    parse_cap,
                    data.len() / 1024
                );
                last_workers = eff_workers;
            }

            // 2. 协议解析 — 单次锁内单次 feed 全量解析 (原实现每包加锁 5 次 +
            //    4 个独立 feed 调用, 逻辑解码协议会重复转换)
            //    帧/CAN/逻辑数据不再逐包 emit, 统一由 Channel 订阅循环周期快照推送
            let feed_start = Instant::now();
            let (frames, can_frames, detection) = if can_parallel {
                metrics_parse
                    .parallel_batches
                    .fetch_add(1, Ordering::Relaxed);
                metrics_parse
                    .max_workers_seen
                    .fetch_max(workers as u64, Ordering::Relaxed);
                if !in_parallel {
                    // 首次进入并行: 接续主引擎内部缓冲里的半个帧
                    parallel.pending = proto2.lock().take_pending();
                    in_parallel = true;
                }
                let (out, detection, timing) = parallel.feed(&proto2, &data, workers).await;
                // 细分计时: split (含锁) / 并行解析 join
                metrics_parse
                    .split_ns
                    .fetch_add(timing.split_ns, Ordering::Relaxed);
                metrics_parse
                    .parse_join_ns
                    .fetch_add(timing.join_ns, Ordering::Relaxed);
                // logic_samples / decoded_events 恒为空 (LogicDecoder 不支持并行), 直通即可
                // 自动通道检测通知 (一次性), 与顺序路径共用
                let detection = if !detection_notified {
                    detection
                } else {
                    None
                };
                (out.frames, out.can_frames, detection)
            } else {
                if in_parallel {
                    // 积压消退, 回落顺序模式: 不完整尾字节喂回主引擎内部缓冲
                    // (零丢失 — 输出恒为空, pending 按构造不含完整帧)
                    let pending = parallel.take_pending();
                    if !pending.is_empty() {
                        let _ = proto2.lock().feed(&pending);
                    }
                    in_parallel = false;
                }
                {
                    let mut p = proto2.lock();
                    // 顺序路径: p.feed 本身耗时计入 parse_join_ns (split_ns 计 0)
                    let t_feed = Instant::now();
                    let out = p.feed(&data);
                    metrics_parse
                        .parse_join_ns
                        .fetch_add(t_feed.elapsed().as_nanos() as u64, Ordering::Relaxed);
                    let frames = out.frames;
                    let can_frames = out.can_frames;
                    let logic_samples = out.logic_samples;
                    let decoded_events = out.decoded_events;
                    // 自动通道检测通知 (一次性), 沿用同一锁 guard
                    let detection = if !detection_notified && p.is_auto_mode() {
                        p.detected_channels()
                    } else {
                        None
                    };
                    if !logic_samples.is_empty() {
                        let mut lb = logic_buffer2.lock();
                        for s in logic_samples {
                            lb.push(s);
                        }
                    }
                    if !decoded_events.is_empty() {
                        let mut db = decoded_buffer2.lock();
                        for e in decoded_events {
                            db.push(e);
                        }
                    }
                    (frames, can_frames, detection)
                }
            };
            acc_bytes += data.len() as u64;
            acc_frames += frames.len() as u64;
            // 本窗口解析出的帧总数 (两条路径都计)
            metrics_parse
                .frames_parsed
                .fetch_add(frames.len() as u64, Ordering::Relaxed);

            if let Some(n) = detection {
                crate::notify::channels_detected(&app2, n);
                detection_notified = true;
            }

            // 2.x CAN 帧处理 (slcan/candleLight) — 非 CAN 协议返回空 Vec
            if !can_frames.is_empty() {
                // push 到 can_buffer + 负载统计器 (仅统计 Rx 方向, 避免发送帧重复计入)
                let mut buf = can_buffer2.lock();
                let mut stats = can_load_stats2.lock();
                for f in can_frames {
                    if f.direction == vofa_next_core::CanDirection::Rx {
                        stats.push(&f);
                    }
                    buf.push(f);
                }
            }

            // 2.2 帧解码器: 喂入原始字节, 更新 decoder_states.last_frame
            //     必须在 evaluate 之前完成, evaluate 阶段从 last_frame 读取输出
            //     返回 has_decoders: 是否存在 FrameDecoder 节点 (供 frames 空时决策)
            let t_dec = Instant::now();
            let has_decoders =
                super::decoder_feed::feed_frame_decoders_cached(&eval_feed, &data, now_us(), &mut dec_cache);
            metrics_parse
                .decoder_feed_ns
                .fetch_add(t_dec.elapsed().as_nanos() as u64, Ordering::Relaxed);
            metrics_parse
                .feed_ns
                .fetch_add(feed_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

            // 3. 递交 eval_task (流水线第二段: push_frame + 图评估 + 派生收集)
            //    帧数据不再逐包 emit, 波形/图输出走 Channel 订阅推送
            if !frames.is_empty() || has_decoders {
                let force_eval = frames.is_empty();
                let t = Instant::now();
                if eval_tx.send(EvalBatch { frames, force_eval }).await.is_err() {
                    log::debug!("评估任务已退出, 停止解析");
                    break;
                }
                metrics_parse
                    .eval_wait_ns
                    .fetch_add(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
            }

            // 5. 统计节流 emit (rx_dropped: 本窗口内 broadcast Lagged 丢弃数)
            let now = Instant::now();
            if now.duration_since(last_stats).as_millis() >= STATS_THROTTLE_MS {
                let _ = app2.emit(
                    "transport:rx",
                    &TransportStats {
                        rx_bytes: acc_bytes,
                        rx_frames: acc_frames,
                        tx_bytes: 0,
                        tx_frames: 0,
                        rx_dropped: metrics_parse.dropped_msgs.swap(0, Ordering::Relaxed),
                    },
                );
                acc_bytes = 0;
                acc_frames = 0;
                last_stats = now;
            }

            metrics_parse.parse_batches.fetch_add(1, Ordering::Relaxed);
        }

        // mpsc 关闭 → eval_tx 随之关闭, eval_task 刷完剩余帧后 emit Disconnected
        log::debug!("解析任务已退出");
    });

    // data_loop: 快速消费 broadcast, 收集原始字节 + 转发到 mpsc (不阻塞在解析上)
    let mut last_report = Instant::now();
    loop {
        match rx.recv().await {
            Ok(data) => {
                metrics.rx_msgs.fetch_add(1, Ordering::Relaxed);
                metrics
                    .rx_bytes
                    .fetch_add(data.len() as u64, Ordering::Relaxed);
                // 原始字节收集前移到此: 一次 memcpy, 不排 parse_task 的队,
                // 即使解析积压 RAWDATA 流也不丢 (通过 Channel 周期推送)
                let t0 = Instant::now();
                raw_data_collector.lock().push_chunk(now_us(), RawDataDirection::Rx, &data);
                metrics
                    .push_chunk_ns
                    .fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
                let t1 = Instant::now();
                if parse_tx.send(data).await.is_err() {
                    log::debug!("解析任务已退出, 停止数据循环");
                    break;
                }
                metrics
                    .send_wait_ns
                    .fetch_add(t1.elapsed().as_nanos() as u64, Ordering::Relaxed);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                log::debug!("数据广播通道已关闭");
                break;
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                // 不再逐条 warn (高码率下每秒数百条会刷屏淹没指标),
                // 汇总进 2s 周期指标 + 100ms transport:rx 统计 (rx_dropped);
                // 仅当 mpsc 深度接近打满 (parse_task 瓶颈, ≥90% 容量) 时立即告警
                metrics.lagged_msgs.fetch_add(n, Ordering::Relaxed);
                metrics.dropped_msgs.fetch_add(n, Ordering::Relaxed);
                let depth = parse_cap - parse_tx.capacity();
                if depth >= parse_cap * 9 / 10 {
                    log::warn!(
                        "数据广播落后 {} 条 (mpsc 深度 {}/{}: parse_task 消费不动)",
                        n,
                        depth,
                        parse_cap
                    );
                }
            }
        }

        // 每 2s 输出一次流水线诊断指标 (仅在广播循环里检查, 有消息才会触发)
        if last_report.elapsed() >= METRICS_REPORT_INTERVAL {
            metrics.report(parse_cap - parse_tx.capacity(), parse_cap);
            last_report = Instant::now();
        }
    }

    // 关闭 mpsc, 等待两段流水线刷完剩余数据
    drop(parse_tx);
    let _ = feed_task.await;
    let _ = eval_task.await;
    log::debug!("数据循环已退出");
}

fn now_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}
