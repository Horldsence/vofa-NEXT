//! 按工作区实际拓扑复现数据链路 — Transport(TestData) → Protocol(JustFloat) →
//! RawData 控件 (ch0 数值边 + 另一直连 rx 字节边)
//!
//! 钉住契约: 字节边 (rx → protocol.in) 必须进 BytePlan, 喂入 JustFloat 帧后
//! 协议解码产帧、数值平面求值, ch0 出现在输出快照 (RawData 数值通道的数据源)。

use std::collections::HashMap;

use app_state::AppState;
use buffer_graph::Edge;
use cmd_graph::apply_tab_graph;
use node_kind::{NodeDef, NodeKind};
use pipeline_data_plane::data_plane::byte_router;
use pipeline_data_plane::decoder_feed::DecoderFeedCache;
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
fn justfloat_frame(values: [f32; 4]) -> Vec<u8> {
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
        let frame = justfloat_frame([i as f32 + 1.0, 2.0, 3.0, 4.0]);
        let summary = byte_router::route_bytes(
            &state.data_plane,
            None,
            "transport-tp",
            &frame,
            0,
            &mut cache,
        )
        .await;
        assert_eq!(summary.frames, 1, "每帧数据应解码出 1 帧 (第 {} 帧)", i);
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
