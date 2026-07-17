use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use vofa_next_buffer::{DataBuffer, NodeGraph};
use vofa_next_core::{ConnectionState, ProtocolConfig};
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

/// 数据循环 — 从传输层接收数据, 喂入协议引擎, 推入缓冲区
pub async fn data_loop(
    app: AppHandle,
    mut rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
    protocol: Arc<Mutex<Box<dyn ProtocolEngine>>>,
    buffer: Arc<Mutex<DataBuffer>>,
) {
    log::info!("数据循环已启动");
    let mut detection_notified = false;

    loop {
        match rx.recv().await {
            Ok(data) => {
                // 1. 发射原始数据 (用于数据显示区)
                let timestamp = now_us();
                let _ = app.emit(
                    "transport:data",
                    &vofa_next_core::RawData {
                        timestamp,
                        data: data.clone(),
                    },
                );

                // 2. 喂入协议引擎, 解析数据帧
                let frames = protocol.lock().feed(&data);
                let frame_count = frames.len() as u64;

                // 2.1 自动通道检测通知 (一次性)
                if !detection_notified {
                    let proto = protocol.lock();
                    if proto.is_auto_mode() {
                        if let Some(n) = proto.detected_channels() {
                            crate::notify::channels_detected(&app, n);
                            detection_notified = true;
                        }
                    }
                }

                // 3. 推入缓冲区 + 发射单帧事件 (供需要逐帧响应的控件)
                if !frames.is_empty() {
                    let mut buf = buffer.lock();
                    for frame in &frames {
                        buf.push_frame(frame);
                    }
                    drop(buf);
                }

                for frame in frames {
                    let _ = app.emit("transport:frame", &frame);
                }

                // 4. 发射统计信息
                let _ = app.emit(
                    "transport:rx",
                    &vofa_next_core::TransportStats {
                        rx_bytes: data.len() as u64,
                        rx_frames: frame_count,
                        tx_bytes: 0,
                        tx_frames: 0,
                    },
                );
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

    let _ = app.emit("transport:state", ConnectionState::Disconnected);
    log::info!("数据循环已退出");
}

fn now_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}
