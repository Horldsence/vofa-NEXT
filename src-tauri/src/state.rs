use parking_lot::Mutex;
use serial_core::{ConnectionState, ProtocolConfig};
use serial_transport::TransportManager;
use serial_vofa::ProtocolEngine;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// 应用全局状态
pub struct AppState {
    /// 传输管理器 (async mutex, 因为 open/send 是异步的)
    pub transport: tokio::sync::Mutex<TransportManager>,
    /// 协议引擎 (sync mutex, feed/encode 是同步的)
    pub protocol: Arc<Mutex<Box<dyn ProtocolEngine>>>,
    /// 当前协议配置
    pub protocol_config: Mutex<ProtocolConfig>,
}

impl AppState {
    pub fn new() -> Self {
        let default_config = ProtocolConfig::default();
        let engine = serial_vofa::create_engine(&default_config);
        Self {
            transport: tokio::sync::Mutex::new(TransportManager::new()),
            protocol: Arc::new(Mutex::new(engine)),
            protocol_config: Mutex::new(default_config),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// 数据循环 — 从传输层接收数据, 喂入协议引擎, 发射事件到前端
pub async fn data_loop(
    app: AppHandle,
    mut rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
    protocol: Arc<Mutex<Box<dyn ProtocolEngine>>>,
) {
    tracing::info!("数据循环已启动");

    loop {
        match rx.recv().await {
            Ok(data) => {
                // 1. 发射原始数据 (用于数据显示区)
                let timestamp = now_us();
                let _ = app.emit(
                    "transport:data",
                    &serial_core::RawData {
                        timestamp,
                        data: data.clone(),
                    },
                );

                // 2. 喂入协议引擎, 发射解析后的数据帧 (用于波形/图表)
                let frames = protocol.lock().feed(&data);
                let frame_count = frames.len() as u64;
                for frame in frames {
                    let _ = app.emit("transport:frame", &frame);
                }

                // 3. 发射统计信息
                let _ = app.emit(
                    "transport:rx",
                    &serial_core::TransportStats {
                        rx_bytes: data.len() as u64,
                        rx_frames: frame_count,
                        tx_bytes: 0,
                        tx_frames: 0,
                    },
                );
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("数据广播通道已关闭");
                break;
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("数据广播落后 {} 条", n);
            }
        }
    }

    let _ = app.emit("transport:state", ConnectionState::Disconnected);
    tracing::info!("数据循环已退出");
}

fn now_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}
