//! CAN 后端抽象 — 用于诊断层 (ISO-TP / UDS / OBD-II / J1939) 接入底层 CAN 帧流
//!
//! 这是 [`vofa-next-protocol`] 中 `ProtocolEngine::feed_can` 的对偶:
//! - `ProtocolEngine` 把"原始字节流 → CanFrame" 的解码做掉
//! - `CanBackend` 把"CanFrame 收发" 暴露成统一接口给上层诊断引擎使用
//!
//! 实现通常由 `vofa-next-automotive` crate 提供,通过桥接
//! `TransportManager` 的原始字节流 + `ProtocolEngine` 编解码完成。
//!
//! 设计为 `async_trait` + `Send + Sync`,可在 tokio task 间共享。

use async_trait::async_trait;
use tokio::sync::broadcast;
use vofa_next_core::{CanFrame, Result};

/// CAN 后端 — 给诊断引擎提供 CanFrame 收发能力的抽象
///
/// 一个 `CanBackend` 实例对应一条活动的 CAN 总线连接 (Slcan / CandleLight / SocketCAN)。
/// 上层诊断引擎通过 [`subscribe_frames`] 获取实时 CanFrame 流,
/// 通过 [`send_frame`] 把诊断请求 (ISO-TP 单帧/多帧,UDS/OBD-II PDU) 推到总线。
#[async_trait]
pub trait CanBackend: Send + Sync {
    /// 发送一帧到 CAN 总线
    ///
    /// 实现内部负责按底层传输格式 (slcan ASCII / candleLight 二进制) 编码,
    /// 并通过 `TransportManager` 的 write_tx 推到设备。
    async fn send_frame(&self, frame: &CanFrame) -> Result<()>;

    /// 订阅 CanFrame 流 — 多消费者语义
    ///
    /// 每次 call 返回独立的 Receiver,与其它订阅者互不干扰。
    /// 实现内部从 TransportManager 的字节流订阅,经 ProtocolEngine 解码后广播。
    fn subscribe_frames(&self) -> broadcast::Receiver<CanFrame>;

    /// 后端名称 (用于日志/调试)
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

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
            direction: vofa_next_core::CanDirection::Tx,
        };
        backend.send_frame(&f).await.unwrap();
        assert_eq!(backend.sent.lock().len(), 1);

        // 模拟 backend 内部把帧推到 broadcast
        let _ = tx.send(f.clone());
        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, 0x123);
        assert_eq!(backend.name(), "mock");
    }
}
