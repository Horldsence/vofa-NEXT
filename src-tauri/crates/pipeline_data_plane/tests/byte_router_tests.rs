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
use buffer_graph::Edge;
use node_engine::BytePlan;
use node_kind::{DecoderBlockDef, FieldType, NodeDef, NodeKind};
use pipeline_data_plane::byte_router::route_bytes;
use pipeline_data_plane::decoder_feed::DecoderFeedCache;
use pipeline_data_plane::DataPlaneState;
use schema_types::{ProtocolConfig, ProtocolSchema, SchemaPreset};
use vofa_core::TransportConfig;

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
        let typed = node_engine::TypedGraph::build(node_map.values().cloned(), edges).unwrap();
        *plane.byte_plan.lock() = BytePlan::build(&typed).unwrap();
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
        vec![
            node("cmd", NodeKind::Sink),
            node("pt", firewater(Some(2))),
            u8_decoder("dec"),
        ],
        vec![
            edge("cmd", "loopbackOut", "pt", "in"),
            edge("cmd", "loopbackOut", "dec", "in"),
        ],
    );
    // FrameDecoder 配置来自 tab 图 (decoder_feed 按 graphs 收集), 注入对应编译图
    let graph =
        node_engine::CompiledGraph::compile("t1".into(), vec![u8_decoder("dec")], vec![]).unwrap();
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
    let graph =
        node_engine::CompiledGraph::compile("t1".into(), vec![u8_decoder("dec")], vec![]).unwrap();
    plane.eval.graphs.lock().insert("t1".into(), graph);

    let mut cache = DecoderFeedCache::new();
    let summary = route_bytes(&plane, None, "tp", b",8", 0, &mut cache).await;
    assert_eq!(summary.frames, 0, "RawData 不产帧");
    assert!(
        summary.decoders_fed,
        "原始字节应沿 out 边透传到 FrameDecoder"
    );
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

/// RawData 节点被用户编辑 decode 块后 (schema preset=Custom, config 仍为 RawData):
/// 走 SchemaEngine 产帧, 不写 source_texts, 原始字节不沿 out 边透传
#[tokio::test]
async fn rawdata_custom_schema_no_text_cache_no_passthrough() {
    // tp.rx → pr.in (RawData + custom schema), pr.out → dec.in (FrameDecoder)
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
                    schema: Some(ProtocolSchema {
                        preset: SchemaPreset::Custom,
                        legacy_config: None,
                        decode: vec![DecoderBlockDef::Field {
                            id: "f1".into(),
                            field_type: FieldType::UInt8,
                            port_name: "v".into(),
                            length_ref: None,
                            match_id: None,
                        }],
                        encode: None,
                    }),
                },
            ),
            u8_decoder("dec"),
        ],
        vec![edge("tp", "rx", "pr", "in"), edge("pr", "out", "dec", "in")],
    );
    // FrameDecoder 配置来自 tab 图 (decoder_feed 按 graphs 收集), 注入对应编译图
    let graph =
        node_engine::CompiledGraph::compile("t1".into(), vec![u8_decoder("dec")], vec![]).unwrap();
    plane.eval.graphs.lock().insert("t1".into(), graph);

    let mut cache = DecoderFeedCache::new();
    let summary = route_bytes(&plane, None, "tp", b",8", 0, &mut cache).await;
    // custom schema 走 SchemaEngine: 无 Header 块, 每字节一帧 (',' 与 '8' 各一帧)
    assert_eq!(summary.frames, 2, "SchemaEngine 应产帧");
    assert!(!summary.decoders_fed, "custom schema 不应沿 out 边透传原文");
    assert!(
        plane.source_texts.lock().get("pr").is_none(),
        "custom schema 不应写 source_texts"
    );
    // 末帧进 source_frames ('8' = 0x38 = 56)
    let sf = plane.source_frames.lock();
    let f = sf.get("pr").expect("pr 应有最新帧");
    assert_eq!(f.channels, vec![56.0]);
}

/// 自动通道检测 (顺序路径): 首帧检测到通道数后, 后端直接把该源 buffer 通道数
/// 对齐到检测值并记录已推送值; 检测值不变时不重复应用
/// (set_channels 会清空数据, 点数持续增长证明未重复清空)
#[tokio::test]
async fn auto_detection_applies_buffer_channels_on_change_only() {
    // tp.rx → pt.in (FireWater 自动检测)
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(Default::default()),
                },
            ),
            node("pt", firewater(None)),
        ],
        vec![edge("tp", "rx", "pt", "in")],
    );
    // 节点创建即按默认通道数对齐 buffer (自动模式待检测)
    assert_eq!(plane.buffer_for("pt").lock().channel_count(), 4);

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
    let buf = plane.buffer_for("pt");
    assert_eq!(buf.lock().channel_count(), 3, "检测值应直接应用到 buffer");
    assert_eq!(buf.lock().point_count(), 1);
    {
        let st = plane.protocol_states.lock().get("pt").unwrap().clone();
        let s = st.lock();
        assert_eq!(s.last_detected_pushed, Some(3), "应记录已推送检测值");
        assert!(s.detection_notified, "系统通知一次性闸应置位");
    }

    // 同值再喂: 不重复应用 (否则 point_count 被清空重置为 1)
    let summary = route_bytes(
        &plane,
        None,
        "tp",
        &firewater_bytes(&[4.0, 5.0, 6.0]),
        0,
        &mut cache,
    )
    .await;
    assert_eq!(summary.frames, 1);
    assert_eq!(buf.lock().point_count(), 2, "同值检测不应重复清空 buffer");
    assert_eq!(buf.lock().channel_count(), 3);
}

/// 自动通道检测 (并行路径): 大批次 + 积压触发并行喂入, par.feed 返回的检测值
/// 同样按变化推送并对齐 buffer
#[tokio::test]
async fn auto_detection_applies_buffer_channels_in_parallel_feed() {
    // tp.rx → pt.in (FireWater 自动检测)
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(Default::default()),
                },
            ),
            node("pt", firewater(None)),
        ],
        vec![edge("tp", "rx", "pt", "in")],
    );
    // 触发并行: depth >= 8 且批次 >= 32KB → workers = 2
    let mut data = Vec::new();
    for i in 0..5000 {
        data.extend_from_slice(format!("{i}.0,2.0,3.0\n").as_bytes());
    }
    assert!(data.len() >= 32 * 1024, "前提: 批次需达到并行字节门槛");

    let mut cache = DecoderFeedCache::new();
    let summary = route_bytes(&plane, None, "tp", &data, 8, &mut cache).await;
    assert_eq!(summary.frames, 5000);
    assert_eq!(
        plane.buffer_for("pt").lock().channel_count(),
        3,
        "并行路径检测值应直接应用到 buffer"
    );
    let st = plane.protocol_states.lock().get("pt").unwrap().clone();
    assert_eq!(st.lock().last_detected_pushed, Some(3));
}

/// 手动通道数: 节点 (重) 建时 buffer 通道数即按配置对齐;
/// 配置变更重建后对齐到新配置生效值 (手动 = 配置值; 自动 = 回默认 4 待重新检测)
#[tokio::test]
async fn buffer_channels_aligned_on_protocol_sync_and_rebuild() {
    // 初始手动 2 通道
    let plane = setup_plane(vec![node("pt", firewater(Some(2)))], vec![]);
    assert_eq!(
        plane.buffer_for("pt").lock().channel_count(),
        2,
        "节点创建即按手动配置对齐 buffer"
    );

    // 配置变更为手动 5 通道 → 重建后 buffer 对齐 5
    plane
        .global_nodes
        .lock()
        .insert("pt".into(), node("pt", firewater(Some(5))));
    plane.sync_protocol_states();
    assert_eq!(plane.buffer_for("pt").lock().channel_count(), 5);

    // 配置变更为自动 → 重建后检测值失效, 回默认 4 待重新检测
    plane
        .global_nodes
        .lock()
        .insert("pt".into(), node("pt", firewater(None)));
    plane.sync_protocol_states();
    assert_eq!(plane.buffer_for("pt").lock().channel_count(), 4);
    let st = plane.protocol_states.lock().get("pt").unwrap().clone();
    assert_eq!(st.lock().last_detected_pushed, None, "重建后推送记录应重置");
}

/// 手动模式下不应触发协议检测推送事件 (detected_channels 在手动模式下返回 None,
/// channels_detection_change 判定为 None→None 不推); 同时 buffer 通道数应保持手动配置值
#[tokio::test]
async fn manual_mode_does_not_emit_channels_detected_event() {
    // tp.rx → pt.in (FireWater 手动 2 通道)
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(Default::default()),
                },
            ),
            node("pt", firewater(Some(2))),
        ],
        vec![edge("tp", "rx", "pt", "in")],
    );
    assert_eq!(plane.buffer_for("pt").lock().channel_count(), 2);

    let mut cache = DecoderFeedCache::new();
    let summary = route_bytes(
        &plane,
        None,
        "tp",
        &firewater_bytes(&[1.0, 2.0]),
        0,
        &mut cache,
    )
    .await;
    assert_eq!(summary.frames, 1);
    let st = plane.protocol_states.lock().get("pt").unwrap().clone();
    assert_eq!(
        st.lock().last_detected_pushed,
        None,
        "手动模式不应记录检测推送值"
    );
    assert_eq!(plane.buffer_for("pt").lock().channel_count(), 2);
}

/// 手动 → 自动切换后首次检测推送: 手动模式无推送记录, 切自动后第一次检测即变化,
/// 应推送且对齐 buffer 到检测值
#[tokio::test]
async fn manual_to_auto_switch_resets_detection_state() {
    // 起始手动 2 通道
    let plane = setup_plane(vec![node("pt", firewater(Some(2)))], vec![]);
    assert_eq!(plane.buffer_for("pt").lock().channel_count(), 2);

    // 配置切换为自动 → sync_protocol_states 重建, 推送记录与 buffer 回默认
    plane
        .global_nodes
        .lock()
        .insert("pt".into(), node("pt", firewater(None)));
    plane.sync_protocol_states();
    assert_eq!(plane.buffer_for("pt").lock().channel_count(), 4);
    {
        let st = plane.protocol_states.lock().get("pt").unwrap().clone();
        assert_eq!(st.lock().last_detected_pushed, None);
    }

    // 切回手动 5 通道
    plane
        .global_nodes
        .lock()
        .insert("pt".into(), node("pt", firewater(Some(5))));
    plane.sync_protocol_states();
    assert_eq!(plane.buffer_for("pt").lock().channel_count(), 5);
}
