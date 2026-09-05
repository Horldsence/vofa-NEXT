//! TransportManager 集成测试 — 多节点管理 / 重连 / 热更新链路配置

use schema_types::{ProtocolConfig, TestDataLink};
use std::time::Duration;
use tokio::sync::broadcast;
use transport_core::TransportManager;
use vofa_core::{ConnectionState, Error, TestDataConfig, TestSignal, TransportConfig};

const fn test_data_config() -> TransportConfig {
    TransportConfig::TestData(TestDataConfig {
        channels: 2,
        sample_rate: 1000.0,
        signal: TestSignal::Sine,
    })
}

async fn open_node(mgr: &mut TransportManager, id: &str) {
    mgr.open(
        id,
        test_data_config(),
        TestDataLink::new(ProtocolConfig::RawData),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn open_close_single_node() {
    let mut mgr = TransportManager::new();
    assert!(mgr.state("a").is_none());
    open_node(&mut mgr, "a").await;
    assert_eq!(mgr.state("a"), Some(ConnectionState::Connected));
    assert!(matches!(
        mgr.config("a"),
        Some(TransportConfig::TestData(_))
    ));

    mgr.close("a");
    assert!(mgr.state("a").is_none());
    assert!(mgr.config("a").is_none());
}

#[tokio::test]
async fn multiple_nodes_are_independent() {
    let mut mgr = TransportManager::new();
    open_node(&mut mgr, "a").await;
    open_node(&mut mgr, "b").await;

    let mut open = mgr.list_open();
    open.sort();
    assert_eq!(open, vec!["a".to_string(), "b".to_string()]);

    // 各自收发统计互不影响
    mgr.send("a", &[1, 2, 3]).unwrap();
    mgr.send("a", &[4]).unwrap();
    mgr.send("b", &[5, 6]).unwrap();
    let sa = mgr.stats("a").unwrap();
    assert_eq!((sa.tx_bytes, sa.tx_frames), (4, 2));
    let sb = mgr.stats("b").unwrap();
    assert_eq!((sb.tx_bytes, sb.tx_frames), (2, 1));

    // rx 统计经轻量句柄上报, 同样按节点隔离
    let live_a = mgr.live_handle("a").unwrap();
    live_a.record_rx(10, 2);
    assert_eq!(mgr.stats("a").unwrap().rx_bytes, 10);
    assert_eq!(mgr.stats("b").unwrap().rx_bytes, 0);

    // 关闭 a 不影响 b
    mgr.close("a");
    assert!(mgr.state("a").is_none());
    assert!(mgr.state("b").is_some());

    mgr.close("b");
    assert!(mgr.list_open().is_empty());
}

#[tokio::test]
async fn reopen_same_id_replaces_connection() {
    let mut mgr = TransportManager::new();
    open_node(&mut mgr, "a").await;
    mgr.send("a", &[1, 2, 3]).unwrap();

    // 重复 open 同 id: 先关闭旧连接, 状态/统计重置
    open_node(&mut mgr, "a").await;
    assert_eq!(mgr.list_open().len(), 1);
    let s = mgr.stats("a").unwrap();
    assert_eq!((s.tx_bytes, s.tx_frames), (0, 0));
}

#[tokio::test]
async fn unknown_node_id_errors() {
    let mut mgr = TransportManager::new();
    let err = mgr.send("nope", &[1]).unwrap_err();
    assert!(matches!(err, Error::PortNotFound(_)));
    assert!(err.to_string().contains("nope"));
    assert!(mgr.state("nope").is_none());
    assert!(mgr.stats("nope").is_none());
    assert!(mgr.config("nope").is_none());
    assert!(mgr.subscribe("nope").is_none());
    // 关闭未知 id 不 panic
    mgr.close("nope");
}

#[tokio::test]
async fn subscribe_receives_test_data() {
    let mut mgr = TransportManager::new();
    open_node(&mut mgr, "a").await;
    open_node(&mut mgr, "b").await;
    let mut rx_a = mgr.subscribe("a").unwrap();
    let mut rx_b = mgr.subscribe("b").unwrap();

    // 连接即生成: 打开后无需任何启停操作, 各节点独立持续产出数据流
    let data = tokio::time::timeout(Duration::from_secs(2), rx_a.recv())
        .await
        .expect("a 应产生数据")
        .unwrap();
    assert!(!data.is_empty());

    let data_b = tokio::time::timeout(Duration::from_secs(2), rx_b.recv())
        .await
        .expect("b 应产生数据")
        .unwrap();
    assert!(!data_b.is_empty());

    // 关闭 a 后其后台任务退出, 通道最终关闭
    mgr.close("a");
}

#[tokio::test]
async fn send_loops_back_to_subscribers() {
    let mut mgr = TransportManager::new();
    open_node(&mut mgr, "a").await;
    let mut rx = mgr.subscribe("a").unwrap();

    // 写入的字节统一回环到本节点接收广播 (transport→transport 路由链不断裂)。
    // 连接即生成: 生成器批次与回环帧共享同一条广播, 逐条等到回环帧为止。
    mgr.send("a", &[0xDE, 0xAD]).unwrap();
    let mut seen = false;
    for _ in 0..200 {
        match tokio::time::timeout(Duration::from_millis(50), rx.recv()).await {
            Ok(Ok(data)) if data == [0xDE, 0xAD] => {
                seen = true;
                break;
            }
            Ok(Ok(_)) => {}
            Ok(Err(_)) | Err(_) => break,
        }
    }
    assert!(seen, "写入应回环到接收广播");
}

#[tokio::test]
async fn test_data_protocol_hot_update() {
    let mut mgr = TransportManager::new();
    mgr.open(
        "a",
        test_data_config(),
        TestDataLink::new(ProtocolConfig::JustFloat { channels: Some(2) }),
    )
    .await
    .unwrap();
    let mut rx = mgr.subscribe("a").unwrap();

    // JustFloat: 帧尾 00 00 80 7f
    let first = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("JustFloat 数据")
        .unwrap();
    assert!(
        first.windows(4).any(|w| w == [0x00, 0x00, 0x80, 0x7f]),
        "应为 JustFloat 格式"
    );

    // 热更新为 FireWater — 无需重建连接, 后续批次应为 ASCII CSV
    mgr.update_link(
        "a",
        TestDataLink::new(ProtocolConfig::FireWater { channels: Some(2) }),
        None,
    )
    .unwrap();
    let mut saw_csv = false;
    for _ in 0..50 {
        let batch = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("热更新后数据")
            .unwrap();
        if batch.last() == Some(&b'\n') && batch.iter().all(u8::is_ascii) {
            saw_csv = true;
            break;
        }
    }
    assert!(saw_csv, "热更新后应生成 FireWater CSV 格式");

    // 未打开的节点热更新报错 (前端据此提示重连)
    assert!(mgr
        .update_link("nope", TestDataLink::new(ProtocolConfig::RawData), None)
        .is_err());
}

#[tokio::test]
async fn test_data_generator_config_hot_update() {
    let mut mgr = TransportManager::new();
    mgr.open(
        "a",
        test_data_config(),
        TestDataLink::new(ProtocolConfig::RawData),
    )
    .await
    .unwrap();
    let mut rx = mgr.subscribe("a").unwrap();

    let first = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("初始生成数据")
        .unwrap();
    assert_eq!(
        first.len() as u64,
        6 * transport_core::test_data::samples_per_message(1_000.0),
        "每帧 2 通道 + 4 字节计数器，消息帧数遵循统一节拍"
    );

    mgr.update_link(
        "a",
        TestDataLink::new(ProtocolConfig::RawData),
        Some(TestDataConfig {
            channels: 5,
            sample_rate: 700_000.0,
            signal: TestSignal::Square,
        }),
    )
    .unwrap();

    let Some(TransportConfig::TestData(runtime_config)) = mgr.config("a") else {
        panic!("TestData 连接应保留运行时配置");
    };
    assert_eq!(runtime_config.channels, 5);
    assert!((runtime_config.sample_rate - 700_000.0).abs() < f32::EPSILON);
    assert_eq!(runtime_config.signal, TestSignal::Square);

    let mut saw_updated = false;
    for _ in 0..50 {
        let batch = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("热更新后数据")
            .unwrap();
        if batch.len() as u64 == 9 * transport_core::test_data::samples_per_message(700_000.0) {
            saw_updated = true;
            break;
        }
    }
    assert!(saw_updated, "运行中的生成器应采用新的通道数");
}

/// TestData 经 schema 热更新: Custom encode 块改变输出格式
#[tokio::test]
async fn test_data_schema_hot_update() {
    use schema_types::{DecoderBlockDef, EncodeBlockDef, FieldType, ProtocolSchema, SchemaPreset};

    let mut mgr = TransportManager::new();
    mgr.open(
        "a",
        test_data_config(),
        TestDataLink::new(ProtocolConfig::JustFloat { channels: Some(1) }),
    )
    .await
    .unwrap();
    let mut rx = mgr.subscribe("a").unwrap();

    // 初始: legacy JustFloat (帧尾 00 00 80 7f)
    let first = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("JustFloat 数据")
        .unwrap();
    assert!(
        first.windows(4).any(|w| w == [0x00, 0x00, 0x80, 0x7f]),
        "应为 JustFloat 格式"
    );

    // 热更新为 Custom schema: encode = AA + float32LE(v) + BB
    // → 每采样帧 6 字节, 批次为帧拼接 (AA 开头, 长度 % 6 == 0)
    let schema = ProtocolSchema {
        preset: SchemaPreset::Custom,
        legacy_config: None,
        decode: vec![DecoderBlockDef::Field {
            id: "f".into(),
            field_type: FieldType::Float32LE,
            port_name: "v".into(),
            length_ref: None,
            match_id: None,
        }],
        encode: Some(vec![
            EncodeBlockDef::ConstHex { hex: "AA".into() },
            EncodeBlockDef::VarRef {
                port_name: "v".into(),
                field_type: FieldType::Float32LE,
            },
            EncodeBlockDef::ConstHex { hex: "BB".into() },
        ]),
    };
    mgr.update_link(
        "a",
        TestDataLink {
            protocol: ProtocolConfig::JustFloat { channels: Some(1) },
            schema: Some(schema),
        },
        None,
    )
    .unwrap();

    let mut saw_schema_frame = false;
    for _ in 0..50 {
        let batch = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("schema 热更新后数据")
            .unwrap();
        if batch.first() == Some(&0xAA) && batch.len() % 6 == 0 && batch.last() == Some(&0xBB) {
            saw_schema_frame = true;
            break;
        }
    }
    assert!(
        saw_schema_frame,
        "热更新后应按 Custom schema encode 块生成帧"
    );

    // 再次热更新回 legacy (schema = None): 输出恢复 JustFloat
    mgr.update_link(
        "a",
        TestDataLink::new(ProtocolConfig::JustFloat { channels: Some(1) }),
        None,
    )
    .unwrap();
    let mut saw_legacy = false;
    for _ in 0..50 {
        let batch = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("回退 legacy 后数据")
            .unwrap();
        if batch.windows(4).any(|w| w == [0x00, 0x00, 0x80, 0x7f]) {
            saw_legacy = true;
            break;
        }
    }
    assert!(saw_legacy, "回退后应恢复 legacy JustFloat 格式");
}

// 抑制 broadcast import 警告 (manager.rs 内部使用)
#[allow(dead_code)]
fn _ensure_broadcast_imported() -> broadcast::Receiver<Vec<u8>> {
    let (tx, _) = broadcast::channel::<Vec<u8>>(1);
    tx.subscribe()
}
