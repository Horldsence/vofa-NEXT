//! JustFloat 全链路数据流测试
//!
//! 钉住契约: Transport(TestData) → Protocol(JustFloat) →
//! RawData 控件 (ch0 数值边 + 另一直连 rx 字节边);
//! 字节边 (rx → protocol.in) 必须进 BytePlan, 喂入 JustFloat 帧后
//! 协议解码产帧、数值平面求值, ch0 出现在输出快照 (RawData 数值通道的数据源)。
//!
//! 数值断言为精确相等 — JustFloat f32 字节回环是位精确的 (clippy::float_cmp 放行)。
#![allow(clippy::float_cmp, clippy::cast_precision_loss)] // 测试数值断言: f32 位精确比较 + i 计数转 f32 的精度损失无意义

use std::collections::HashMap;

use app_state::AppState;
use buffer_graph::Edge;
use data_bus::TopicKey;
use data_plane::data_plane::byte_router;
use data_plane::decoder_feed::DecoderFeedCache;
use graph_ops::apply_tab_graph;
use kind::{MathOp, NodeDef, NodeKind};
use schema_types::ProtocolConfig;
use vofa_core::config::{TestDataConfig, TransportConfig};

fn transport_node(id: &str) -> NodeDef {
    NodeDef {
        id: id.into(),
        tab_id: "default".into(),
        kind: NodeKind::Transport {
            config: TransportConfig::TestData(TestDataConfig::default()),
        },
    }
}

fn protocol_node(id: &str) -> NodeDef {
    NodeDef {
        id: id.into(),
        tab_id: "default".into(),
        kind: NodeKind::Protocol {
            config: ProtocolConfig::JustFloat { channels: None },
            convert_to: None,
            schema: None,
        },
    }
}

fn sink_node(id: &str) -> NodeDef {
    NodeDef {
        id: id.into(),
        tab_id: "default".into(),
        kind: NodeKind::Sink,
    }
}

/// JustFloat 帧: 4 × f32 LE + 帧尾 [0x00, 0x00, 0x80, 0x7f]
fn justfloat_frame(values: &[f32]) -> Vec<u8> {
    let mut data = Vec::new();
    for v in values {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data.extend_from_slice(&[0x00, 0x00, 0x80, 0x7f]);
    data
}

#[tokio::test]
async fn justfloat_chain_decodes_frames_into_value_plane() {
    let state = AppState::new();
    apply_tab_graph(
        &state,
        None,
        "default".into(),
        vec![
            transport_node("transport-tp"),
            protocol_node("protocol-jf"),
            sink_node("w-raw"),
        ],
        vec![
            Edge {
                id: "e-byte".into(),
                source: "transport-tp".into(),
                source_handle: "rx".into(),
                target: "protocol-jf".into(),
                target_handle: "in".into(),
            },
            Edge {
                id: "e-ch0".into(),
                source: "protocol-jf".into(),
                source_handle: "ch0".into(),
                target: "w-raw".into(),
                target_handle: "src:protocol-jf:ch0".into(),
            },
        ],
        HashMap::new(),
        None,
        None,
        None,
    )
    .await
    .expect("提交图应成功");

    // 字节边进了 BytePlan: transport.rx 的下游含 protocol.in
    let routes: Vec<String> = state
        .data_plane
        .byte_plan
        .lock()
        .routes_for("transport-tp")
        .iter()
        .map(|r| format!("{}:{}", r.target, r.target_handle))
        .collect();
    assert!(
        routes.iter().any(|r| r == "protocol-jf:in"),
        "protocol.in 应在字节路由表中, 实际: {routes:?}"
    );

    // 喂入 3 帧 JustFloat 数据
    let mut cache = DecoderFeedCache::new();
    for i in 0..3u32 {
        let frame = justfloat_frame(&[i as f32 + 1.0, 2.0, 3.0, 4.0]);
        let summary = byte_router::route_bytes(
            &state.data_plane,
            None,
            "transport-tp",
            &frame,
            0,
            &mut cache,
        )
        .await;
        assert_eq!(summary.frames, 1, "每帧数据应解码出 1 帧 (第 {i} 帧)");
    }

    // 数值平面: ch0..ch3 出现在输出快照 (RawData 数值通道读取的数据源)
    let snap = state.data_plane.eval.output_snapshot.lock();
    let ports = snap
        .values
        .get("protocol-jf")
        .unwrap_or_else(|| panic!("协议节点应有数值输出, 实际: {:?}", snap.values.keys()));
    for ch in ["ch0", "ch1", "ch2", "ch3"] {
        assert!(ports.contains_key(ch), "输出快照应含 {ch}, 实际: {ports:?}");
    }
    assert_eq!(ports["ch0"], 3.0, "ch0 应为最后一帧的第一个通道值");
}

#[tokio::test]
async fn auto_channels_above_four_flow_through_math_topic() {
    let state = AppState::new();
    apply_tab_graph(
        &state,
        None,
        "default".into(),
        vec![
            transport_node("transport-tp"),
            protocol_node("protocol-jf"),
            NodeDef {
                id: "math-high".into(),
                tab_id: "default".into(),
                kind: NodeKind::Math {
                    op: MathOp::Add,
                    input_count: 1,
                },
            },
            sink_node("number-high"),
        ],
        vec![
            Edge {
                id: "e-byte".into(),
                source: "transport-tp".into(),
                source_handle: "rx".into(),
                target: "protocol-jf".into(),
                target_handle: "in".into(),
            },
            Edge {
                id: "e-high".into(),
                source: "protocol-jf".into(),
                source_handle: "ch7".into(),
                target: "math-high".into(),
                target_handle: "in0".into(),
            },
            Edge {
                id: "e-display".into(),
                source: "math-high".into(),
                source_handle: "result".into(),
                target: "number-high".into(),
                target_handle: "value".into(),
            },
        ],
        HashMap::new(),
        None,
        None,
        None,
    )
    .await
    .expect("自动通道图应编译");

    let mut channel_rx = state
        .data_plane
        .eval
        .data_bus
        .subscribe(TopicKey::new("protocol-jf", "ch7"), 16)
        .await
        .expect("ch7 topic");
    let mut math_rx = state
        .data_plane
        .eval
        .data_bus
        .subscribe(TopicKey::new("math-high", "result"), 16)
        .await
        .expect("math topic");

    let mut cache = DecoderFeedCache::new();
    let frame = justfloat_frame(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
    byte_router::route_bytes(
        &state.data_plane,
        None,
        "transport-tp",
        &frame,
        0,
        &mut cache,
    )
    .await;

    let channel_batch = channel_rx.recv().await.expect("ch7 sample");
    let math_batch = math_rx.recv().await.expect("math sample");
    assert_eq!(
        channel_batch.samples.last().map(|sample| sample.value),
        Some(8.0)
    );
    assert_eq!(
        math_batch.samples.last().map(|sample| sample.value),
        Some(8.0)
    );
}
