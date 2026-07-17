use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use vofa_next_buffer::{DataBuffer, NodeGraph};
use vofa_next_core::{ConnectionState, DataFrame, ProtocolConfig, RawData, TransportStats};
use vofa_next_protocol::ProtocolEngine;
use vofa_next_transport::TransportManager;

/// 应用全局状态
pub struct AppState {
    /// 传输管理器 (async mutex, 因为 open/send 是异步的)
    pub transport: tokio::sync::Mutex<TransportManager>,
    /// 协议引擎 (sync mutex, feed/encode 是同步的)
    pub protocol: Arc<Mutex<Box<dyn ProtocolEngine>>>,
    /// 当前协议配置
    pub protocol_config: Mutex<ProtocolConfig>,
    /// 多通道数据缓冲区
    pub buffer: Arc<Mutex<DataBuffer>>,
    /// 节点图 (数据路由)
    pub graph: Arc<Mutex<NodeGraph>>,
}

impl AppState {
    pub fn new() -> Self {
        let default_config = ProtocolConfig::default();
        let engine = vofa_next_protocol::create_engine(&default_config);
        Self {
            transport: tokio::sync::Mutex::new(TransportManager::new()),
            protocol: Arc::new(Mutex::new(engine)),
            protocol_config: Mutex::new(default_config),
            buffer: Arc::new(Mutex::new(DataBuffer::new(10_000, 4))),
            graph: Arc::new(Mutex::new(NodeGraph::new())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// 统计节流间隔 (毫秒) — 避免高波特率下每包都 emit 统计
const STATS_THROTTLE_MS: u128 = 100;

/// 数据循环 — 快速消费传输层 broadcast, 转发到解析 task
///
/// 架构 (A+B+C 重构):
/// - data_loop (本函数): 只做 recv + mpsc.send, 最快消费 broadcast 避免 Lagged
/// - parse_task: 从 mpsc 收数据, 做 协议解析 + buffer.push + 批量 emit
///   - 批量 emit: `transport:frames` (数组) 替代每帧一次 `transport:frame`
///   - 统计节流: STATS_THROTTLE_MS 内累积, 一次性 emit
pub async fn data_loop(
    app: AppHandle,
    mut rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
    protocol: Arc<Mutex<Box<dyn ProtocolEngine>>>,
    buffer: Arc<Mutex<DataBuffer>>,
) {
    log::info!("数据循环已启动");

    // 解析 task 用的 mpsc (大容量缓冲, 避免反压到 data_loop)
    let (parse_tx, mut parse_rx) = mpsc::channel::<Vec<u8>>(2048);
    let app2 = app.clone();
    let proto2 = protocol.clone();
    let buf2 = buffer.clone();

    let parse_task = tokio::spawn(async move {
        let mut detection_notified = false;
        let mut frame_batch: Vec<DataFrame> = Vec::with_capacity(128);
        let mut last_stats = Instant::now();
        let mut acc_bytes: u64 = 0;
        let mut acc_frames: u64 = 0;

        while let Some(data) = parse_rx.recv().await {
            // 1. emit 原始数据 (用于数据显示区)
            let _ = app2.emit(
                "transport:data",
                &RawData {
                    timestamp: now_us(),
                    data: data.clone(),
                },
            );

            // 2. 协议解析
            let frames = proto2.lock().feed(&data);
            acc_bytes += data.len() as u64;
            acc_frames += frames.len() as u64;

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

            // 3. 推入缓冲区
            if !frames.is_empty() {
                let mut buf = buf2.lock();
                for f in &frames {
                    buf.push_frame(f);
                }
                drop(buf);
                frame_batch.extend(frames);
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
