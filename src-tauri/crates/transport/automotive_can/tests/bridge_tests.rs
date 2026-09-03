//! `automotive_can` 集成测试 — BridgeCanBackend 编解码桥接

use can_types::{CanDirection, CanFrame};
use tokio::sync::{broadcast, mpsc};
use transport_core::CanBackend;

use automotive_can::{BackendKind, BridgeCanBackend};

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

    let (byte_tx, _) = broadcast::channel::<Vec<u8>>(64);
    let (write_tx, _write_rx) = mpsc::channel(16);
    let backend2 = BridgeCanBackend::spawn(write_tx, byte_tx.subscribe(), BackendKind::CandleLight);
    assert_eq!(backend2.name(), "CandleBridge");
    assert_eq!(backend2.kind(), BackendKind::CandleLight);
    backend2.shutdown();
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

#[test]
fn backend_kind_equality_and_debug() {
    assert_eq!(BackendKind::Slcan, BackendKind::Slcan);
    assert_ne!(BackendKind::Slcan, BackendKind::CandleLight);
    let s = format!("{:?}", BackendKind::Slcan);
    assert!(s.contains("Slcan"));
}
