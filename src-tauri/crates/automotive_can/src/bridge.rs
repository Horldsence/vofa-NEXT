//! `BridgeCanBackend` — 把 `TransportManager` 的字节流与 `ProtocolEngine` 包装成
//! 统一的 `CanBackend` trait。

use async_trait::async_trait;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use can_types::{CanDirection, CanFrame};
use vofa_core::{Error, Result};
use error::TransportError;
use protocol_can_bridge::{CandleEngine, SlcanEngine};
use protocol_engine::ProtocolEngine;
use transport_core::CanBackend;

/// 桥接器配置 — 选择底层 CAN 协议编解码引擎
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Lawicel ASCII (slcan) — 串口 USB-CAN
    Slcan,
    /// candleLight (GSUSB) — 原生 USB
    CandleLight,
}

/// 桥接的 CAN 后端 — 把 transport 的字节流和 ProtocolEngine 包装成 CanBackend
///
/// 内部状态:
/// - `write_tx`: 把编码后的字节送到 transport (设备方向)
/// - `frame_tx`: 解码后的 CanFrame 广播 (上层订阅方向)
/// - `engine`: 编解码引擎,持有缓冲状态 (Mutex 保护,因 task 与 send_frame 都会访问)
/// - `cancel`: 任务取消标志
pub struct BridgeCanBackend {
    write_tx: mpsc::Sender<Vec<u8>>,
    frame_tx: broadcast::Sender<CanFrame>,
    engine: Arc<Mutex<Box<dyn ProtocolEngine + Send>>>,
    cancel: Arc<AtomicBool>,
    kind: BackendKind,
}

impl BridgeCanBackend {
    /// 创建新的桥接器并 spawn 后台解码任务
    ///
    /// `byte_rx`: 从 `TransportManager::subscribe()` 获取的字节流订阅
    /// `write_tx`: `TransportManager::write_tx` 的克隆 (用于发送)
    /// `kind`: 选择 Slcan / CandleLight 编解码
    pub fn spawn(
        write_tx: mpsc::Sender<Vec<u8>>,
        byte_rx: broadcast::Receiver<Vec<u8>>,
        kind: BackendKind,
    ) -> Self {
        let engine: Box<dyn ProtocolEngine + Send> = match kind {
            BackendKind::Slcan => Box::new(SlcanEngine::new()),
            BackendKind::CandleLight => Box::new(CandleEngine::new()),
        };
        let engine = Arc::new(Mutex::new(engine));
        let (frame_tx, _) = broadcast::channel(1024);
        let cancel = Arc::new(AtomicBool::new(false));

        // Spawn 解码任务
        let engine_task = engine.clone();
        let frame_tx_task = frame_tx.clone();
        let cancel_task = cancel.clone();
        tokio::spawn(async move {
            let mut byte_rx = byte_rx;
            loop {
                if cancel_task.load(Ordering::Relaxed) {
                    break;
                }
                // 用 recv_timeout 而非 recv,以便周期性检查 cancel
                match tokio::time::timeout(std::time::Duration::from_millis(100), byte_rx.recv())
                    .await
                {
                    Err(_) => {} // timeout,继续循环检查 cancel
                    Ok(Err(_)) => break, // channel 关闭
                    Ok(Ok(bytes)) => {
                        if bytes.is_empty() {
                            continue;
                        }
                        let frames = {
                            let mut eng = engine_task.lock();
                            eng.feed(&bytes).can_frames
                        };
                        for frame in frames {
                            // 发送失败说明没有订阅者,忽略即可
                            let _ = frame_tx_task.send(frame);
                        }
                    }
                }
            }
            log::debug!("BridgeCanBackend 解码任务退出 (kind={kind:?})");
        });

        Self {
            write_tx,
            frame_tx,
            engine,
            cancel,
            kind,
        }
    }

    /// 停止后台解码任务
    pub fn shutdown(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// 引擎种类
    pub const fn kind(&self) -> BackendKind {
        self.kind
    }
}

impl Drop for BridgeCanBackend {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[async_trait]
impl CanBackend for BridgeCanBackend {
    async fn send_frame(&self, frame: &CanFrame) -> Result<()> {
        // 强制方向为 Tx (上层调用 send_frame 都是发送)
        let tx_frame = CanFrame {
            direction: CanDirection::Tx,
            ..frame.clone()
        };
        let encoded = {
            let mut eng = self.engine.lock();
            eng.encode_can(&tx_frame)
        };
        if encoded.is_empty() {
            return Err(Error::Transport(TransportError::CanEncode {
                id: frame.id,
                details: format!("{:?} 引擎无法编码", self.kind),
            }));
        }
        self.write_tx
            .send(encoded)
            .await
            .map_err(|_| TransportError::CanSend(std::io::Error::other("channel closed")))?;
        Ok(())
    }

    fn subscribe_frames(&self) -> broadcast::Receiver<CanFrame> {
        self.frame_tx.subscribe()
    }

    fn name(&self) -> &str {
        match self.kind {
            BackendKind::Slcan => "SlcanBridge",
            BackendKind::CandleLight => "CandleBridge",
        }
    }
}