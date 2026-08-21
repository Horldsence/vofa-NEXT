//! CanBackend trait 契约测试 — 用 MockCanBackend 验证 trait 可实现并按约定工作

use async_trait::async_trait;
use tokio::sync::broadcast;
use transport_core::CanBackend;
use can_types::{CanDirection, CanFrame};
use vofa_core::Result;

/// 一个最简的内存 CanBackend,用于测试 trait 契约
struct MockCanBackend {
    tx: broadcast::Sender<CanFrame>,
    sent: parking_lot::Mutex<Vec<CanFrame>>,
}

#[async_trait]
impl CanBackend for MockCanBackend {
    async fn send_frame(&self, frame: &CanFrame) -> Result<()> {
        self.sent.lock().push(frame.clone());
        Ok(())
    }
    fn subscribe_frames(&self) -> broadcast::Receiver<CanFrame> {
        self.tx.subscribe()
    }
    fn name(&self) -> &str {
        "mock"
    }
}

#[tokio::test]
async fn trait_can_be_implemented_and_used() {
    let (tx, _) = broadcast::channel(16);
    let backend = MockCanBackend {
        tx: tx.clone(),
        sent: parking_lot::Mutex::new(Vec::new()),
    };
    let mut rx = backend.subscribe_frames();
    let f = CanFrame {
        timestamp: 0,
        id: 0x123,
        extended: false,
        rtr: false,
        dlc: 1,
        data: vec![0xAA],
        direction: CanDirection::Tx,
    };
    backend.send_frame(&f).await.unwrap();
    assert_eq!(backend.sent.lock().len(), 1);

    // 模拟 backend 内部把帧推到 broadcast
    let _ = tx.send(f.clone());
    let received = rx.recv().await.unwrap();
    assert_eq!(received.id, 0x123);
    assert_eq!(backend.name(), "mock");
}