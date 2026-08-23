//! byte_router 模块集成测试
//!
//! 数据平面字节路由端到端验证: Transport.rx → Protocol.in / FrameDecoder.in /
//! convert_to 重编码 → 下游 Protocol.in / Transport.tx 等路径。
//!
//! 注: 这些测试不能作为 `pipeline_data_plane` 的内联测试 — 内联测试需通过
//! dev-dep 反向依赖 `app_state`, cargo 在 dev-dep 循环下不统一
//! `data_plane::DataPlaneState` 与 `pipeline_data_plane::DataPlaneState`
//! 两个同源码类型, 测试编译失败 (E0308), 故以 tests/ 集成测试形式存在。

use app_state::AppState;
use pipeline_data_plane::byte_router::route_bytes;
use pipeline_data_plane::decoder_feed::DecoderFeedCache;
use pipeline_data_plane::DataPlaneState;
use buffer_graph::Edge;
use schema_types::ProtocolConfig;
use vofa_core::TransportConfig;
use node_engine::BytePlan;
use node_kind::{DecoderBlockDef, FieldType, NodeDef, NodeKind};

fn node(id: &str, kind: NodeKind) -> NodeDef {
    NodeDef {
        id: id.into(),
        tab_id: "t1".into(),
        kind,
    }
}

fn edge(src: &str, src_h: &str, tgt: &str, tgt_h: &str) -> Edge {
    Edge {
        id: format!("{}-{}", src, tgt),
        source: src.into(),
        source_handle: src_h.into(),
        target: tgt.into(),
        target_handle: tgt_h.into(),
    }
}

/// "," (0x2C) 帧头 + 1 字节无符号字段 (输出端口 "v")
fn u8_decoder(id: &str) -> NodeDef {
    node(
        id,
        NodeKind::FrameDecoder {
            blocks: vec![
                DecoderBlockDef::Header {
                    id: "h1".into(),
                    hex: "2C".into(),
                    match_id: None,
                },
                DecoderBlockDef::Field {
                    id: "f1".into(),
                    field_type: FieldType::UInt8,
                    port_name: "v".into(),
                    length_ref: None,
                    match_id: None,
                },
            ],
            enable_valid: false,
            enable_frame_count: false,
            enable_last_timestamp: false,
            enable_fps: false,
            loopback: false,
        },
    )
}

fn firewater(channels: Option<usize>) -> NodeKind {
    NodeKind::Protocol {
        config: ProtocolConfig::FireWater { channels },
        convert_to: None,
        schema: None,
    }
}

/// 内存构造数据平面: 全局节点表 + BytePlan + protocol_states 同步
fn setup_plane(nodes: Vec<NodeDef>, edges: Vec<Edge>) -> DataPlaneState {
    let state = AppState::new();
    let plane = state.data_plane.clone();
    {
        let mut g = plane.global_nodes.lock();
        for n in nodes {
            g.insert(n.id.clone(), n);
        }
        let node_map = g.clone();
        *plane.byte_plan.lock() = BytePlan::build(&node_map, &edges).unwrap();
    }
    plane.sync_protocol_states();
    plane
}

fn firewater_bytes(channels: &[f32]) -> Vec<u8> {
    let s = channels
        .iter()
        .map(|v| format!("{}", v))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}\n", s).into_bytes()
}

#[tokio::test]
async fn transport_to_protocol_feeds_source_frames() {
    // tp.rx → pt.in (FireWater 3 通道)
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(Default::default()),
                },
            ),
            node("pt", firewater(Some(3))),
        ],
        vec![edge("tp", "rx", "pt", "in")],
    );
    let mut cache = DecoderFeedCache::new();
    let summary = route_bytes(
        &plane,
        None,
        "tp",
        &firewater_bytes(&[1.0, 2.0, 3.0]),
        0,
        &mut cache,
    )
    .await;
    assert_eq!(summary.frames, 1);
    let sf = plane.source_frames.lock();
    let f = sf.get("pt").expect("pt 应有最新帧");
    assert_eq!(f.channels, vec![1.0, 2.0, 3.0]);
    // 按源 DataBuffer 实例也应有 1 帧
    assert_eq!(plane.buffer_for("pt").lock().point_count(), 1);
}

#[tokio::test]
async fn convert_to_chain_reencodes_downstream() {
    // tp.rx → pa.in (FireWater), pa.out → pb.in (JustFloat)
    // pa 配置 convert_to = JustFloat: pa 解析出的帧按 JustFloat 重编码喂给 pb
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(Default::default()),
                },
            ),
            node(
                "pa",
                NodeKind::Protocol {
                    config: ProtocolConfig::FireWater { channels: Some(2) },
                    convert_to: Some(ProtocolConfig::JustFloat { channels: Some(2) }),
                    schema: None,
                },
            ),
            node(
                "pb",
                NodeKind::Protocol {
                    config: ProtocolConfig::JustFloat { channels: Some(2) },
                    convert_to: None,
                    schema: None,
                },
            ),
        ],
        vec![edge("tp", "rx", "pa", "in"), edge("pa", "out", "pb", "in")],
    );
    let mut cache = DecoderFeedCache::new();
    let summary = route_bytes(
        &plane,
        None,
        "tp",
        &firewater_bytes(&[4.0, 5.0]),
        0,
        &mut cache,
    )
    .await;
    assert_eq!(summary.frames, 2, "pa/pb 各解析出一帧");
    let sf = plane.source_frames.lock();
    assert_eq!(sf.get("pa").unwrap().channels, vec![4.0, 5.0]);
    assert_eq!(sf.get("pb").unwrap().channels, vec![4.0, 5.0]);
}

#[tokio::test]
async fn inject_routes_to_multiple_downstreams() {
    // widget loopbackOut → pt.in (Protocol) + dec.in (FrameDecoder)
    let plane = setup_plane(
        vec![node("cmd", NodeKind::Sink), node("pt", firewater(Some(2))), u8_decoder("dec")],
        vec![
            edge("cmd", "loopbackOut", "pt", "in"),
            edge("cmd", "loopbackOut", "dec", "in"),
        ],
    );
    // FrameDecoder 配置来自 tab 图 (decoder_feed 按 graphs 收集), 注入对应编译图
    let graph =
        node_engine::CompiledGraph::compile("t1".into(), vec![u8_decoder("dec")], vec![])
            .unwrap();
    plane.eval.graphs.lock().insert("t1".into(), graph);
    plane
        .eval
        .graphs_version
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let mut cache = DecoderFeedCache::new();
    let summary = route_bytes(
        &plane,
        None,
        "cmd",
        &firewater_bytes(&[7.0, 8.0]),
        0,
        &mut cache,
    )
    .await;
    assert_eq!(summary.frames, 1, "FireWater 解析出一帧");
    assert!(summary.decoders_fed, "FrameDecoder 应被喂入");
    // 协议分支
    assert_eq!(
        plane.source_frames.lock().get("pt").unwrap().channels,
        vec![7.0, 8.0]
    );
    // 解码器分支: ',' 帧头后的字段字节 ('8' = 0x38 = 56)
    let ds = plane.eval.decoder_states.lock();
    let parser = ds.get("dec").expect("dec parser 应存在");
    assert_eq!(parser.last_frame.outputs.get("v"), Some(&56.0));
}

/// RawData 协议: 不产帧; 原始字节 UTF-8 lossy 解码进 source_texts + 沿 out 边透传下游
#[tokio::test]
async fn rawdata_protocol_caches_text_and_passthrough_out() {
    // tp.rx → pr.in (RawData), pr.out → dec.in (FrameDecoder)
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(Default::default()),
                },
            ),
            node(
                "pr",
                NodeKind::Protocol {
                    config: ProtocolConfig::RawData,
                    convert_to: None,
                    schema: None,
                },
            ),
            u8_decoder("dec"),
        ],
        vec![edge("tp", "rx", "pr", "in"), edge("pr", "out", "dec", "in")],
    );
    // FrameDecoder 配置来自 tab 图 (decoder_feed 按 graphs 收集), 注入对应编译图
    let graph = node_engine::CompiledGraph::compile("t1".into(), vec![u8_decoder("dec")], vec![])
        .unwrap();
    plane.eval.graphs.lock().insert("t1".into(), graph);

    let mut cache = DecoderFeedCache::new();
    let summary = route_bytes(&plane, None, "tp", b",8", 0, &mut cache).await;
    assert_eq!(summary.frames, 0, "RawData 不产帧");
    assert!(summary.decoders_fed, "原始字节应沿 out 边透传到 FrameDecoder");
    // source_texts 缓存原始字节的 UTF-8 文本
    assert_eq!(
        plane.source_texts.lock().get("pr").map(String::as_str),
        Some(",8")
    );
    // 透传字节被下游解码器消费: ',' 帧头后的字段字节 ('8' = 0x38 = 56)
    {
        let ds = plane.eval.decoder_states.lock();
        let parser = ds.get("dec").expect("dec parser 应存在");
        assert_eq!(parser.last_frame.outputs.get("v"), Some(&56.0));
    }

    // UTF-8 lossy: 非法字节序列替换为 U+FFFD (覆盖写, latest-value)
    let summary = route_bytes(&plane, None, "tp", b"\xff", 0, &mut cache).await;
    assert_eq!(summary.frames, 0);
    assert_eq!(
        plane.source_texts.lock().get("pr").map(String::as_str),
        Some("\u{FFFD}")
    );
}
