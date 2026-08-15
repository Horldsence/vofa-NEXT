//! CAN 后端桥接实现 — 把 `TransportManager` 的原始字节流 + `ProtocolEngine`
//! 的 CAN 帧编解码,组装成符合 `CanBackend` trait 的统一接口。
//!
//! 桥接器内部 spawn 一个 tokio task,从 transport 的字节 broadcast 订阅,
//! 喂入 SlcanEngine/CandleEngine,把解码出的 CanFrame 广播给上层诊断引擎。
//!
//! 发送方向:把 CanFrame 经 encode_can 编码为字节,通过 transport 的 write_tx
//! 推到设备。

use async_trait::async_trait;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use vofa_next_core::{CanDirection, CanFrame, Error, Result};
use vofa_next_protocol::{CandleEngine, ProtocolEngine, SlcanEngine};
use vofa_next_transport::CanBackend;

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
    /// `byte_rx`: 从 TransportManager::subscribe() 获取的字节流订阅
    /// `write_tx`: TransportManager::write_tx 的克隆 (用于发送)
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
                    Err(_) => {}         // timeout,继续循环检查 cancel
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
            return Err(Error::Transport(format!(
                "{:?} 引擎无法编码 CanFrame (id=0x{:X})",
                self.kind, frame.id
            )));
        }
        self.write_tx
            .send(encoded)
            .await
            .map_err(|e| Error::Transport(format!("CAN 后端发送失败: {e}")))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    /// 构造一个测试用 byte 广播通道并 spawn Slcan 桥接
    fn spawn_slcan_bridge() -> (
        BridgeCanBackend,
        broadcast::Sender<Vec<u8>>,
        mpsc::Receiver<Vec<u8>>,
    ) {
        let (byte_tx, _) = broadcast::channel(64);
        let (write_tx, write_rx) = mpsc::channel(16);
        let byte_rx = byte_tx.subscribe();
        let backend = BridgeCanBackend::spawn(write_tx, byte_rx, BackendKind::Slcan);
        (backend, byte_tx, write_rx)
    }

    #[tokio::test]
    async fn slcan_bridge_decodes_received_bytes() {
        let (backend, byte_tx, _write_rx) = spawn_slcan_bridge();
        let mut frame_rx = backend.subscribe_frames();

        // 喂入 slcan 数据帧: t123401020304\r
        let _ = byte_tx.send(b"t123401020304\r".to_vec());

        // 等待解码任务产出 CanFrame
        let frame = tokio::time::timeout(std::time::Duration::from_millis(500), frame_rx.recv())
            .await
            .expect("timeout 等待 CanFrame")
            .expect("channel 关闭");

        assert_eq!(frame.id, 0x123);
        assert_eq!(frame.dlc, 4);
        assert_eq!(frame.data, vec![0x01, 0x02, 0x03, 0x04]);
        assert_eq!(frame.direction, CanDirection::Rx);

        backend.shutdown();
    }

    #[tokio::test]
    async fn slcan_bridge_encodes_outgoing_frames() {
        let (backend, _byte_tx, mut write_rx) = spawn_slcan_bridge();

        let frame = CanFrame {
            timestamp: 0,
            id: 0x123,
            extended: false,
            rtr: false,
            dlc: 4,
            data: vec![0x01, 0x02, 0x03, 0x04],
            direction: CanDirection::Tx,
        };
        backend.send_frame(&frame).await.unwrap();

        let encoded = tokio::time::timeout(std::time::Duration::from_millis(500), write_rx.recv())
            .await
            .expect("timeout 等待编码字节")
            .expect("channel 关闭");

        // SlcanEngine::encode_can 应输出 "t123401020304\r"
        assert_eq!(encoded, b"t123401020304\r");

        backend.shutdown();
    }

    #[tokio::test]
    async fn candle_bridge_decodes_received_bytes() {
        let (byte_tx, _) = broadcast::channel(64);
        let (write_tx, _write_rx) = mpsc::channel(16);
        let byte_rx = byte_tx.subscribe();
        let backend = BridgeCanBackend::spawn(write_tx, byte_rx, BackendKind::CandleLight);
        let mut frame_rx = backend.subscribe_frames();

        // 构造一个 24 字节 candleLight RX 帧 (id=0x123, dlc=4, data=[0x01,0x02,0x03,0x04])
        let mut pkt = vec![0u8; 24];
        pkt[0] = 0x11; // CAND_CMD_RX
        pkt[8..12].copy_from_slice(&0x123u32.to_le_bytes());
        pkt[12] = 4;
        pkt[16..20].copy_from_slice(&[0x01, 0x02, 0x03, 0x04]);
        let _ = byte_tx.send(pkt);

        let frame = tokio::time::timeout(std::time::Duration::from_millis(500), frame_rx.recv())
            .await
            .expect("timeout 等待 CanFrame")
            .expect("channel 关闭");

        assert_eq!(frame.id, 0x123);
        assert_eq!(frame.dlc, 4);
        assert_eq!(frame.data, vec![0x01, 0x02, 0x03, 0x04]);

        backend.shutdown();
    }

    #[tokio::test]
    async fn candle_bridge_encodes_outgoing_frames() {
        let (byte_tx, _) = broadcast::channel(64);
        let (write_tx, mut write_rx) = mpsc::channel(16);
        let byte_rx = byte_tx.subscribe();
        let backend = BridgeCanBackend::spawn(write_tx, byte_rx, BackendKind::CandleLight);

        let frame = CanFrame {
            timestamp: 0,
            id: 0x123,
            extended: false,
            rtr: false,
            dlc: 4,
            data: vec![0x01, 0x02, 0x03, 0x04],
            direction: CanDirection::Tx,
        };
        backend.send_frame(&frame).await.unwrap();

        let encoded = tokio::time::timeout(std::time::Duration::from_millis(500), write_rx.recv())
            .await
            .expect("timeout 等待编码字节")
            .expect("channel 关闭");

        // 应为 24 字节 candleLight TX 帧
        assert_eq!(encoded.len(), 24);
        assert_eq!(encoded[0], 0x12); // CAND_CMD_TX
        let can_id = u32::from_le_bytes([encoded[8], encoded[9], encoded[10], encoded[11]]);
        assert_eq!(can_id, 0x123);
        assert_eq!(encoded[12], 4);
        assert_eq!(&encoded[16..20], &[0x01, 0x02, 0x03, 0x04]);

        backend.shutdown();
    }

    #[tokio::test]
    async fn backend_name_reflects_kind() {
        let (backend, _byte_tx, _write_rx) = spawn_slcan_bridge();
        assert_eq!(backend.name(), "SlcanBridge");
        assert_eq!(backend.kind(), BackendKind::Slcan);
        backend.shutdown();
    }

    #[tokio::test]
    async fn multiple_subscribers_each_get_frames() {
        let (backend, byte_tx, _write_rx) = spawn_slcan_bridge();
        let mut rx1 = backend.subscribe_frames();
        let mut rx2 = backend.subscribe_frames();

        let _ = byte_tx.send(b"t123401020304\r".to_vec());

        let f1 = tokio::time::timeout(std::time::Duration::from_millis(500), rx1.recv())
            .await
            .expect("rx1 timeout")
            .expect("rx1 closed");
        let f2 = tokio::time::timeout(std::time::Duration::from_millis(500), rx2.recv())
            .await
            .expect("rx2 timeout")
            .expect("rx2 closed");

        assert_eq!(f1.id, 0x123);
        assert_eq!(f2.id, 0x123);

        backend.shutdown();
    }

    #[tokio::test]
    async fn shutdown_stops_decode_task() {
        let (byte_tx, _) = broadcast::channel(64);
        let (write_tx, _write_rx) = mpsc::channel(16);
        let byte_rx = byte_tx.subscribe();
        let backend = BridgeCanBackend::spawn(write_tx, byte_rx, BackendKind::Slcan);

        backend.shutdown();
        // 给 task 一点时间退出
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        // 推一帧,订阅者不应收到 (任务已停止)
        let mut rx = backend.subscribe_frames();
        let _ = byte_tx.send(b"t123401020304\r".to_vec());
        let result = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;
        assert!(result.is_err(), "shutdown 后不应再收到 CanFrame");
    }
}
