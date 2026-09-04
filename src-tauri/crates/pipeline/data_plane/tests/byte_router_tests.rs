//! byte_router 模块集成测试
//!
//! 数据平面字节路由端到端验证: Transport.rx → Protocol.in / FrameDecoder.in /
//! convert_to 重编码 → 下游 Protocol.in / Transport.tx 等路径。
//!
//! 注: 这些测试不能作为 `data_plane` 的内联测试 — 内联测试需通过
//! dev-dep 反向依赖 `app_state`, cargo 在 dev-dep 循环下不统一
//! `data_plane::DataPlaneState` 与 `app_state` 经由 `data_plane` re-export 的 `DataPlaneState`
//! 两个同源码类型, 测试编译失败 (E0308), 故以 tests/ 集成测试形式存在。

use app_state::AppState;
use buffer_graph::Edge;
use data_plane::byte_router::route_bytes;
use data_plane::decoder_feed::DecoderFeedCache;
use data_plane::DataPlaneState;
use engine::BytePlan;
use kind::{DecoderBlockDef, FieldType, NodeDef, NodeKind};
use schema_types::{ProtocolConfig, ProtocolSchema, SchemaPreset, TestDataLink};
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
        id: format!("{src}-{tgt}"),
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

const fn firewater(channels: Option<usize>) -> NodeKind {
    NodeKind::Protocol {
        config: ProtocolConfig::FireWater { channels },
        convert_to: None,
        schema: None,
    }
}

/// 内存构造数据平面: 全局节点表 + BytePlan + protocol_states 同步
fn setup_plane(nodes: Vec<NodeDef>, edges: Vec<Edge>) -> DataPlaneState {
    let state = AppState::new();
    let plane = state.data_plane;
    {
        let mut g = plane.global_nodes.lock();
        for n in nodes {
            g.insert(n.id.clone(), n);
        }
        let node_map = g.clone();
        let typed = engine::TypedGraph::build(node_map.values().cloned(), edges).unwrap();
        *plane.byte_plan.lock() = BytePlan::build(&typed).unwrap();
    }
    plane.sync_protocol_states();
    plane
}

fn firewater_bytes(channels: &[f32]) -> Vec<u8> {
    let s = channels
        .iter()
        .map(|v| format!("{v}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{s}\n").into_bytes()
}

#[tokio::test]
async fn transport_to_protocol_feeds_source_frames() {
    // tp.rx → pt.in (FireWater 3 通道)
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(vofa_core::TestDataConfig::default()),
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
        None,
    )
    .await;
    plane.flush_eval();
    assert_eq!(summary.frames, 1);
    let sf = plane.source_frames.lock();
    let f = sf.get("pt").expect("pt 应有最新帧");
    assert_eq!(f.channels, vec![1.0, 2.0, 3.0]);
    // 按源 DataBuffer 实例也应有 1 帧
    assert_eq!(plane.buffer_for("pt").lock().point_count(), 1);
}

#[tokio::test]
async fn test_data_frames_use_configured_sample_clock() {
    let test_config = vofa_core::TestDataConfig {
        channels: 1,
        sample_rate: 700_000.0,
        ..vofa_core::TestDataConfig::default()
    };
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(test_config),
                },
            ),
            node("pt", firewater(Some(1))),
        ],
        vec![edge("tp", "rx", "pt", "in")],
    );
    let mut cache = DecoderFeedCache::new();
    route_bytes(&plane, None, "tp", b"1\n2\n3\n4\n", 0, &mut cache, None).await;
    plane.flush_eval();

    let timestamps = plane.buffer_for("pt").lock().get_recent(4).timestamps;
    assert_eq!(timestamps.len(), 4);
    assert!(timestamps.windows(2).all(|pair| pair[0] < pair[1]));
    assert!((0.004..=0.005).contains(&-timestamps[0]));
    assert!(timestamps[3].abs() < f64::EPSILON);

    // 热更新只替换后续采样间隔，不把采样时钟重新锚定到墙钟。
    {
        let mut nodes = plane.global_nodes.lock();
        let transport = nodes.get_mut("tp").unwrap();
        let NodeKind::Transport {
            config: TransportConfig::TestData(config),
        } = &mut transport.kind
        else {
            panic!("tp should remain TestData");
        };
        config.sample_rate = 350_000.0;
    }
    route_bytes(&plane, None, "tp", b"5\n6\n7\n", 0, &mut cache, None).await;
    plane.flush_eval();
    let absolute = plane.buffer_for("pt").lock().get_window_raw(-1_000.0, 0.0);
    let timestamps = absolute
        .timestamps
        .iter()
        .map(|offset_ms| offset_ms * 1_000.0)
        .collect::<Vec<_>>();
    assert_eq!(timestamps.len(), 7);
    let hot_update_step = timestamps[4] - timestamps[3];
    assert!(
        (2.0..=4.0).contains(&hot_update_step),
        "热更新边界应立即使用约 2.86µs 的新间隔，实际 {hot_update_step}µs"
    );
    assert!(timestamps[4..]
        .windows(2)
        .all(|pair| (2.0..=4.0).contains(&(pair[1] - pair[0]))));
}

#[tokio::test]
async fn connected_test_data_runtime_clock_overrides_stale_graph_config() {
    let stale_config = vofa_core::TestDataConfig {
        channels: 1,
        sample_rate: 1_000.0,
        ..vofa_core::TestDataConfig::default()
    };
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(stale_config),
                },
            ),
            node("pt", firewater(Some(1))),
        ],
        vec![edge("tp", "rx", "pt", "in")],
    );
    let runtime_config = vofa_core::TestDataConfig {
        channels: 1,
        sample_rate: 700_000.0,
        ..vofa_core::TestDataConfig::default()
    };
    plane
        .transport
        .lock()
        .await
        .open(
            "tp",
            TransportConfig::TestData(runtime_config),
            TestDataLink::new(ProtocolConfig::FireWater { channels: Some(1) }),
        )
        .await
        .unwrap();

    let mut cache = DecoderFeedCache::new();
    route_bytes(&plane, None, "tp", b"1\n2\n3\n4\n", 0, &mut cache, None).await;
    plane.flush_eval();

    let timestamps = plane.buffer_for("pt").lock().get_recent(4).timestamps;
    assert_eq!(timestamps.len(), 4);
    assert!(
        (0.004..=0.005).contains(&-timestamps[0]),
        "必须使用连接运行时的 700 kHz，而不是图中的旧 1 kHz: {timestamps:?}"
    );
    plane.transport.lock().await.close("tp");
}

#[tokio::test]
async fn serial_frames_are_spaced_by_wire_bitrate_instead_of_batch_arrival() {
    let plane = setup_plane(
        vec![
            node(
                "serial",
                NodeKind::Transport {
                    config: TransportConfig::Serial(vofa_core::SerialConfig {
                        baud_rate: 115_200,
                        ..vofa_core::SerialConfig::default()
                    }),
                },
            ),
            node("pt", firewater(Some(1))),
        ],
        vec![edge("serial", "rx", "pt", "in")],
    );
    let mut cache = DecoderFeedCache::new();
    // 8N1 下每帧 "N\n" 占 20 bit，115200 baud 对应 5760 frame/s。
    route_bytes(&plane, None, "serial", b"1\n2\n3\n4\n", 0, &mut cache, None).await;
    plane.flush_eval();

    let timestamps = plane.buffer_for("pt").lock().get_recent(4).timestamps;
    assert_eq!(timestamps.len(), 4);
    assert!(timestamps.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(
        (0.520..=0.522).contains(&-timestamps[0]),
        "串口批内四帧应按线速铺开，实际 {timestamps:?}"
    );
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
                    config: TransportConfig::TestData(vofa_core::TestDataConfig::default()),
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
        None,
    )
    .await;
    plane.flush_eval();
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
        engine::CompiledGraph::compile("t1".into(), vec![u8_decoder("dec")], vec![]).unwrap();
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
        None,
    )
    .await;
    plane.flush_eval();
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
                    config: TransportConfig::TestData(vofa_core::TestDataConfig::default()),
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
        engine::CompiledGraph::compile("t1".into(), vec![u8_decoder("dec")], vec![]).unwrap();
    plane.eval.graphs.lock().insert("t1".into(), graph);

    let mut cache = DecoderFeedCache::new();
    let summary = route_bytes(&plane, None, "tp", b",8", 0, &mut cache, None).await;
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
    let summary = route_bytes(&plane, None, "tp", b"\xff", 0, &mut cache, None).await;
    assert_eq!(summary.frames, 0);
    assert_eq!(
        plane.source_texts.lock().get("pr").map(String::as_str),
        Some("\u{FFFD}")
    );
}

/// RawData + convert_to: 不产帧、重编码产物为空 → 原始字节仍沿 out 边透传
/// (修复点: 旧逻辑 convert_to 分支吞掉空产物后原文被静默丢弃)
#[tokio::test]
async fn rawdata_with_convert_to_still_passthrough_out() {
    use vofa_core::config::TestDataConfig;
    // tp.rx → pr.in (RawData + convert_to=FireWater), pr.out → dec.in (FrameDecoder)
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(TestDataConfig::default()),
                },
            ),
            node(
                "pr",
                NodeKind::Protocol {
                    config: ProtocolConfig::RawData,
                    convert_to: Some(ProtocolConfig::FireWater { channels: Some(2) }),
                    schema: None,
                },
            ),
            u8_decoder("dec"),
        ],
        vec![edge("tp", "rx", "pr", "in"), edge("pr", "out", "dec", "in")],
    );
    let graph =
        engine::CompiledGraph::compile("t1".into(), vec![u8_decoder("dec")], vec![]).unwrap();
    plane.eval.graphs.lock().insert("t1".into(), graph);

    let mut cache = DecoderFeedCache::new();
    let summary = route_bytes(&plane, None, "tp", b",8", 0, &mut cache, None).await;
    assert_eq!(summary.frames, 0, "RawData 不产帧");
    assert!(
        summary.decoders_fed,
        "设置 convert_to 后原文仍应透传而非丢弃"
    );
    assert_eq!(
        plane.source_texts.lock().get("pr").map(String::as_str),
        Some(",8"),
        "文本缓存照常写入"
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
                    config: TransportConfig::TestData(vofa_core::TestDataConfig::default()),
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
        engine::CompiledGraph::compile("t1".into(), vec![u8_decoder("dec")], vec![]).unwrap();
    plane.eval.graphs.lock().insert("t1".into(), graph);

    let mut cache = DecoderFeedCache::new();
    let summary = route_bytes(&plane, None, "tp", b",8", 0, &mut cache, None).await;
    plane.flush_eval();
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
                    config: TransportConfig::TestData(vofa_core::TestDataConfig::default()),
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
        None,
    )
    .await;
    plane.flush_eval();
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
        None,
    )
    .await;
    plane.flush_eval();
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
                    config: TransportConfig::TestData(vofa_core::TestDataConfig::default()),
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
    let summary = route_bytes(&plane, None, "tp", &data, 8, &mut cache, None).await;
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
                    config: TransportConfig::TestData(vofa_core::TestDataConfig::default()),
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
        None,
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

/// 端到端复现: 真实 TestData 生成器 (700k sps) + read_task 式 64KB 合批 +
/// route_bytes 全链路。缓冲最近 200ms 窗口的相邻时间戳间隔应 ≈1.43µs;
/// 若出现 ~ms 级间隔, 说明采样时钟恢复在真实管线路径上失效 (波形阶梯的根因)
#[tokio::test]
async fn test_data_700k_full_pipeline_has_per_sample_timestamps() {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    let test_config = vofa_core::TestDataConfig {
        channels: 4,
        sample_rate: 700_000.0,
        signal: vofa_core::TestSignal::Sine,
    };
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(test_config.clone()),
                },
            ),
            node("pt", firewater(Some(4))),
        ],
        vec![edge("tp", "rx", "pt", "in")],
    );

    let (_write_tx, data_tx, _cancel, running, _notify, _runtime_tx) =
        transport_core::test_data::spawn(
            test_config,
            TestDataLink {
                protocol: ProtocolConfig::FireWater { channels: Some(4) },
                schema: None,
            },
        )
        .unwrap();
    running.store(true, Ordering::Relaxed);
    let mut rx = data_tx.subscribe();

    // 与 read_task 相同的合批策略 (目标 64KB); 广播 Lagged 计数 (workspace
    // 并行负载下可能溢出 — 丢消息产生诚实缺口, 见下方断言的容差)
    let mut cache = DecoderFeedCache::new();
    let mut lost_messages = 0_u64;
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        let first = match rx.recv().await {
            Ok(first) => first,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                lost_messages += n;
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        };
        let mut data = first;
        while data.len() < 64 * 1024 {
            match rx.try_recv() {
                Ok(mut next) => data.append(&mut next),
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
                    lost_messages += n;
                }
                Err(_) => break,
            }
        }
        route_bytes(&plane, None, "tp", &data, 0, &mut cache, None).await;
    }
    running.store(false, Ordering::Relaxed);

    let buf = plane.buffer_for("pt");
    let b = buf.lock();
    let window = b.get_window_raw(-200.0, 0.0);
    assert!(
        window.raw_window_points > 10_000,
        "200ms 窗口应有数万点, 实际 {}",
        window.raw_window_points
    );
    let max_gap_ms = window
        .timestamps
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .fold(0.0_f64, f64::max);
    // 无丢弃: 严格逐样本 (≈0.0014ms); 广播 Lagged 的丢消息产生诚实缺口
    // (单消息 ≈350 样本 ≈0.5ms) — 缺口必须与丢失量自洽
    let expected_max_gap_ms =
        f64::from(u32::try_from(lost_messages).unwrap_or(u32::MAX)).mul_add(0.5, 0.05);
    assert!(
        max_gap_ms < expected_max_gap_ms,
        "采样时钟恢复失效: 相邻时间戳最大间隔 {max_gap_ms}ms \
         (期望 ≈0.0014ms, 丢失 {lost_messages} 条消息容差 {expected_max_gap_ms:.2}ms)"
    );

    // 诊断: min-max 实时窗口的实际输出形态 (700k, 100ms 窗口, 预算 10000, 4 通道)
    let selection = buffer_databuffer::WaveformSeriesSelection {
        channels: vec![0, 1, 2, 3],
        derived: vec![],
    };
    let mm = b.get_window_min_max(-100.0, 0.0, 10_000, &selection);
    eprintln!(
        "min-max 输出: 列数 {}, 原始点 {}, CH3 点数 {}",
        mm.timestamps.len(),
        mm.raw_window_points,
        mm.channels[3].len()
    );
    let ch3 = &mm.channels[3];
    eprintln!("CH3 前 40 点: {:?}", &ch3[..ch3.len().min(40)]);
    // 平台检测: 最长连续近恒定 (|Δ| < 0.5) 段长度
    let mut longest = 1usize;
    let mut run = 1usize;
    for pair in ch3.windows(2) {
        if (pair[1] - pair[0]).abs() < 0.5 {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 1;
        }
    }
    eprintln!("CH3 最长平台段: {longest} 列");
    // 原始数据对照: 同窗口 CH3 的原始最大/最小间隔
    let raw = b.get_window_raw(-100.0, 0.0);
    let raw3 = &raw.channels[3];
    eprintln!("原始 CH3 前 20 点: {:?}", &raw3[..raw3.len().min(20)]);
}

/// 完整复现真实运行条件: 700k 摄入 + 并发 min-max 流 (16ms detail + 100ms
/// 概览, 与前端订阅一致) 抢缓冲锁, 持续 3 秒后检查输出是否出现块状跳变
#[tokio::test]
async fn test_data_700k_under_stream_pressure_stays_smooth() {
    run_stream_pressure_case(ProtocolConfig::FireWater { channels: Some(4) }, 4).await;
}

/// JustFloat 路径同样复现 (用户实际配置): 700k 4 通道正弦 + 并发流压力
#[tokio::test]
async fn test_data_700k_justfloat_under_stream_pressure_stays_smooth() {
    run_stream_pressure_case(ProtocolConfig::JustFloat { channels: Some(4) }, 4).await;
}

async fn run_stream_pressure_case(protocol: ProtocolConfig, channels: usize) {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    let test_config = vofa_core::TestDataConfig {
        channels,
        sample_rate: 700_000.0,
        signal: vofa_core::TestSignal::Sine,
    };
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(test_config.clone()),
                },
            ),
            node(
                "pt",
                NodeKind::Protocol {
                    config: protocol.clone(),
                    convert_to: None,
                    schema: None,
                },
            ),
        ],
        vec![edge("tp", "rx", "pt", "in")],
    );

    let (_write_tx, data_tx, _cancel, running, _notify, _runtime_tx) =
        transport_core::test_data::spawn(
            test_config,
            TestDataLink {
                protocol,
                schema: None,
            },
        )
        .unwrap();
    running.store(true, Ordering::Relaxed);
    let mut rx = data_tx.subscribe();

    // 模拟应用真实流负载: 3 个订阅 (detail + 概览 + 主源), 16ms 节奏。
    // 修复前每次 drain 持锁计算 (debug 下 20ms+), 3 流锁占空比 >150% → 摄入饿死;
    // 修复后锁内只剩快照拷贝, 摄入应保持
    let stop_streams = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut stream_handles = Vec::new();
    for _ in 0..3 {
        let plane = plane.clone();
        let stop = stop_streams.clone();
        stream_handles.push(tokio::task::spawn_blocking(move || {
            let selection = buffer_databuffer::WaveformSeriesSelection {
                channels: vec![0, 1, 2, 3],
                derived: vec![],
            };
            while !stop.load(Ordering::Relaxed) {
                // 与修复后的流路径一致: 锁内仅快照拷贝, 计算在锁外
                let snapshot = {
                    let b = plane.buffer_for("pt");
                    let b = b.lock();
                    b.snapshot_window(-100.0, 0.0, &selection)
                };
                let _ = snapshot.into_min_max(10_000);
                std::thread::sleep(Duration::from_millis(16));
            }
        }));
    }

    let mut cache = DecoderFeedCache::new();
    let mut total_frames = 0_u64;
    let mut lagged = 0_u64;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        // 与 read_task 一致: Lagged 计数后继续 (不中断摄入)
        let first = match rx.recv().await {
            Ok(data) => data,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                lagged += n;
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        };
        let mut data = first;
        while data.len() < 64 * 1024 {
            match rx.try_recv() {
                Ok(mut next) => data.append(&mut next),
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
                    lagged += n;
                }
                Err(_) => break,
            }
        }
        total_frames += route_bytes(&plane, None, "tp", &data, 0, &mut cache, None)
            .await
            .frames;
    }
    running.store(false, Ordering::Relaxed);
    stop_streams.store(true, Ordering::Relaxed);
    for handle in stream_handles {
        let _ = handle.await;
    }

    // 摄入完整性: 流压力不应造成广播溢出丢帧
    eprintln!("摄入 {total_frames} 帧, 丢弃 {lagged} 条消息");
    assert!(
        lagged == 0,
        "流压力下广播溢出丢弃 {lagged} 条消息 (摄入被锁竞争/计算拖垮)"
    );

    let buf = plane.buffer_for("pt");
    let b = buf.lock();
    let selection = buffer_databuffer::WaveformSeriesSelection {
        channels: vec![0, 1, 2, 3],
        derived: vec![],
    };
    let mm = b.get_window_min_max(-100.0, 0.0, 10_000, &selection);
    let ch3 = &mm.channels[3];
    // 干净正弦 (4Hz, 幅度 125): 相邻列最大 |Δ| ≈ 3.1/ms × 列间距 (~0.04ms) ≈ 0.13
    let max_step = ch3
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0_f32, f32::max);
    let jump_count = ch3
        .windows(2)
        .filter(|pair| (pair[1] - pair[0]).abs() > 5.0)
        .count();
    eprintln!(
        "压力下 min-max: 列数 {}, CH3 最大相邻步进 {max_step}, 跳变(>5)次数 {jump_count}",
        mm.timestamps.len()
    );
    eprintln!("CH3 前 40 点: {:?}", &ch3[..ch3.len().min(40)]);
    assert!(
        jump_count == 0,
        "出现 {jump_count} 次相邻跳变 (最大 {max_step}), 正弦应平滑"
    );
}

/// 手工持续压力门禁：64 MB JustFloat 字节流（4 通道，20 B/帧）必须以 >10 MB/s
/// 穿过完整解析/记录路径；同时让概览线程持续读取金字塔，验证 L0 滚动覆盖后
/// 主图与全局概览的公共时间轴都不回跳、不塌缩。
///
/// 运行：`cargo test --release -p data_plane --test byte_router_tests
/// sustained_justfloat_over_10mbps_keeps_waveform_timeline_stable -- --ignored --nocapture`
#[tokio::test]
#[ignore = "manual sustained throughput gate; run with --release"]
#[allow(clippy::cast_precision_loss)] // 有界测试规模的吞吐与相位计算
async fn sustained_justfloat_over_10mbps_keeps_waveform_timeline_stable() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    const CHANNELS: usize = 4;
    const FRAMES_PER_CHUNK: usize = 3_200;
    const CHUNKS: usize = 1_000;
    const BYTES_PER_FRAME: usize = CHANNELS * 4 + 4;

    let protocol = ProtocolConfig::JustFloat {
        channels: Some(CHANNELS),
    };
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(vofa_core::TestDataConfig {
                        channels: CHANNELS,
                        sample_rate: 700_000.0,
                        signal: vofa_core::TestSignal::Sine,
                    }),
                },
            ),
            node(
                "pt",
                NodeKind::Protocol {
                    config: protocol,
                    convert_to: None,
                    schema: None,
                },
            ),
        ],
        vec![edge("tp", "rx", "pt", "in")],
    );

    let mut chunk = Vec::with_capacity(FRAMES_PER_CHUNK * BYTES_PER_FRAME);
    for frame in 0..FRAMES_PER_CHUNK {
        let phase = frame as f32 * 0.017;
        for channel in 0..CHANNELS {
            let value = (channel as f32)
                .mul_add(0.7, phase)
                .sin()
                .mul_add(125.0, 128.0);
            chunk.extend_from_slice(&value.to_le_bytes());
        }
        chunk.extend_from_slice(&[0x00, 0x00, 0x80, 0x7f]);
    }
    assert_eq!(chunk.len(), FRAMES_PER_CHUNK * BYTES_PER_FRAME);

    // 与真实 UI 同时存在的全局概览订阅：采集期间周期性做预算快照与 min-max。
    let stop_overview = Arc::new(AtomicBool::new(false));
    let overview_stop = stop_overview.clone();
    let overview_plane = plane.clone();
    let overview = tokio::task::spawn_blocking(move || {
        while !overview_stop.load(Ordering::Relaxed) {
            let snapshot = {
                let buffer = overview_plane.buffer_for("pt");
                let buffer = buffer.lock();
                buffer.snapshot_all_budget(2_000)
            };
            let window = snapshot.into_min_max(2_000);
            assert!(
                window.timestamps.windows(2).all(|pair| pair[0] <= pair[1]),
                "压力期间概览时间轴发生回跳"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    });

    let mut cache = DecoderFeedCache::new();
    let started = Instant::now();
    let mut parsed_frames = 0_u64;
    for _ in 0..CHUNKS {
        parsed_frames += route_bytes(&plane, None, "tp", &chunk, 1_024, &mut cache, None)
            .await
            .frames;
    }
    let elapsed = started.elapsed();
    stop_overview.store(true, Ordering::Relaxed);
    overview.await.expect("概览压力线程不应失败");

    let total_bytes = chunk.len() * CHUNKS;
    let throughput_mbps = total_bytes as f64 / elapsed.as_secs_f64() / 1_000_000.0;
    eprintln!(
        "持续压力: {:.1} MB / {:.3}s = {:.1} MB/s, 解析 {} 帧",
        total_bytes as f64 / 1_000_000.0,
        elapsed.as_secs_f64(),
        throughput_mbps,
        parsed_frames
    );
    assert_eq!(parsed_frames, (FRAMES_PER_CHUNK * CHUNKS) as u64);
    assert!(
        throughput_mbps > 10.0,
        "完整解析/记录路径吞吐仅 {throughput_mbps:.1} MB/s，未达到 10 MB/s 门禁"
    );

    let buffer = plane.buffer_for("pt");
    let buffer = buffer.lock();
    assert!(buffer.storage_overflow() > 0, "压力量必须触发 L0 滚动覆盖");

    let overview = buffer.snapshot_all_budget(2_000).into_min_max(2_000);
    assert!(overview.buffer_tier > 0, "全局概览必须由金字塔层服务");
    assert!(overview.timestamps.len() > 2, "全局概览不得塌缩/消失");
    assert!(
        overview
            .timestamps
            .windows(2)
            .all(|pair| pair[0] <= pair[1]),
        "最终概览时间轴发生回跳"
    );

    let selection = buffer_databuffer::WaveformSeriesSelection {
        channels: (0..CHANNELS).collect(),
        derived: vec![],
    };
    let detail = buffer
        .snapshot_window_budget(-2_000.0, 0.0, &selection, 12_000)
        .into_min_max(12_000);
    assert!(detail.buffer_tier > 0, "高密度 2s 主图应走预算金字塔层");
    assert!(detail.timestamps.len() > 2, "主图窗口不得塌缩/消失");
    assert!(
        detail.timestamps.windows(2).all(|pair| pair[0] <= pair[1]),
        "最终主图时间轴发生回跳"
    );
}

/// 评估队列有界: 小批过载受 256 批上限约束，记录平面不丢帧。
/// (摄入/评估解耦后的显式降级语义 — 丢最旧保最新, 波形尾部始终可见)
#[tokio::test]
async fn frame_queue_overflow_keeps_newest_batches() {
    let plane = setup_plane(
        vec![
            node(
                "tp",
                NodeKind::Transport {
                    config: TransportConfig::TestData(vofa_core::TestDataConfig::default()),
                },
            ),
            node("pt", firewater(Some(3))),
        ],
        vec![edge("tp", "rx", "pt", "in")],
    );
    let mut cache = DecoderFeedCache::new();
    // 禁用本次喂入的协作预算让出，单线程 worker 在断言前不会消费。
    tokio::task::unconstrained(async {
        #[allow(clippy::cast_precision_loss)] // 0..300 的小整数转 f32 精确
        for i in 0..300u32 {
            let v = i as f32 + 1.0;
            route_bytes(
                &plane,
                None,
                "tp",
                &firewater_bytes(&[v, v, v]),
                0,
                &mut cache,
                None,
            )
            .await;
        }
    })
    .await;
    let diag = plane.eval_diagnostics();
    assert_eq!(diag.queued_batches, 256);
    assert_eq!(diag.queued_frames, 256);
    assert_eq!(diag.dropped_frames, 44);
    plane.flush_eval();
    assert_eq!(plane.eval_diagnostics().completed_frames, 256);
    assert_eq!(plane.eval_diagnostics().queued_estimated_bytes, 0);
    // 不变量 3: 记录平面无条件入库，求值丢弃不影响原始波形。
    assert_eq!(plane.buffer_for("pt").lock().point_count(), 300);
    // 不变量 5: 求值队列丢最旧保最新 — source_frames 为最新批, 且缺口被记账
    // (flush_eval 消费缺口并复位有状态算子; 本例无状态图, 复位为空操作)
    let sf = plane.source_frames.lock();
    let f = sf.get("pt").expect("pt 应有最新帧");
    assert_eq!(f.channels, vec![300.0, 300.0, 300.0]);
}
