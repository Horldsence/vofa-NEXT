//! 字节路由 — 沿全局 BytePlan 把字节事件推送到所有下游
//!
//! 入口 [`route_bytes`]: 以 `source_id` (Transport 节点 / widget loopbackOut /
//! Protocol 节点 convert 链) 为源, 查 `BytePlan::routes_for` 逐个下游分发:
//! - Protocol 节点 `in`: 喂入解析引擎 (保留合批后的顺序/并行解析),
//!   产帧 → [`super::frame_dispatch::on_frames`] 写 source_frames + 触发数值平面;
//!   can/logic/decoded 旁路进全局缓冲; 若有 convert_to, 输出引擎 encode_frame
//!   重编码 → 沿本节点 `out` 边递归下推 (BytePlan 拓扑序保证无环, 另有深度上限兜底)
//! - FrameDecoder 节点 `in`/`loopbackIn`: 走 feed_one_decoder 语义 (按边路由)
//! - Transport 节点 `tx`: registry.send (协议转换回注 / 命令发送落地)

use tauri::AppHandle;
use vofa_next_nodes::{
    NodeKind, FRAME_DECODER_IN_HANDLE, LOOPBACK_IN_HANDLE, PROTOCOL_IN_HANDLE,
    TRANSPORT_TX_HANDLE,
};

use super::DataPlaneState;
use crate::pipeline::decoder_feed::DecoderFeedCache;
use crate::pipeline::feed_parallel::workers_needed;

/// convert 链递归深度上限 (BytePlan 已保证 DAG, 此为防御性兜底)
const MAX_ROUTE_DEPTH: usize = 16;

/// 路由结果摘要 (统计 + 触发决策)
#[derive(Default)]
pub struct RouteSummary {
    /// 本次路由解析出的数据帧总数 (所有命中 Protocol 节点合计)
    pub frames: u64,
    /// 是否有 FrameDecoder 被喂入 (调用方据此做快照评估)
    pub decoders_fed: bool,
    /// 数值平面评估累计耗时 ns (观测用)
    pub eval_ns: u64,
}

/// 沿全局 BytePlan 推送字节 (事件驱动入口)
///
/// - `source_id`: 字节源节点 (Transport 节点 id / widget loopbackOut 所在 widget id)
/// - `depth_hint`: 源端积压深度 (并行解析判定用; 命令注入路径传 0)
/// - `app`: 自动通道检测通知用 (测试/无界面路径传 None)
pub async fn route_bytes(
    plane: &DataPlaneState,
    app: Option<&AppHandle>,
    source_id: &str,
    data: &[u8],
    depth_hint: usize,
    dec_cache: &mut DecoderFeedCache,
) -> RouteSummary {
    let mut summary = RouteSummary::default();
    route_inner(plane, app, source_id, data, depth_hint, dec_cache, &mut summary, 0).await;
    summary
}

#[allow(clippy::too_many_arguments)]
async fn route_inner(
    plane: &DataPlaneState,
    app: Option<&AppHandle>,
    source_id: &str,
    data: &[u8],
    depth_hint: usize,
    dec_cache: &mut DecoderFeedCache,
    summary: &mut RouteSummary,
    depth: usize,
) {
    if depth > MAX_ROUTE_DEPTH {
        log::warn!("字节路由深度超限 ({}), 丢弃 {} 字节", source_id, data.len());
        return;
    }
    // 路由表快照 (锁即刻释放, 下游分发不持 byte_plan 锁)
    let routes: Vec<_> = plane.byte_plan.lock().routes_for(source_id).to_vec();
    for route in routes {
        let kind = plane
            .global_nodes
            .lock()
            .get(&route.target)
            .map(|n| n.kind.clone());
        let Some(kind) = kind else { continue };
        match (&kind, route.target_handle.as_str()) {
            (NodeKind::Protocol { .. }, PROTOCOL_IN_HANDLE) => {
                feed_protocol(plane, app, &route.target, data, depth_hint, dec_cache, summary, depth)
                    .await;
            }
            (NodeKind::FrameDecoder { .. }, FRAME_DECODER_IN_HANDLE | LOOPBACK_IN_HANDLE) => {
                let ts = vofa_next_core::now_us();
                if crate::pipeline::decoder_feed::feed_decoder_by_id(
                    &plane.eval,
                    &route.target,
                    data,
                    ts,
                    dec_cache,
                ) {
                    summary.decoders_fed = true;
                }
            }
            (NodeKind::Transport { .. }, TRANSPORT_TX_HANDLE) => {
                // 协议转换回注 / 命令发送落地 — try_lock 避免与 open 的长持锁互等
                match plane.transport.try_lock() {
                    Ok(m) => {
                        if let Err(e) = m.send(&route.target, data) {
                            log::debug!("字节路由发送失败 ({}): {}", route.target, e);
                        }
                    }
                    Err(_) => log::warn!("传输注册表锁忙, 丢弃发往 {} 的 {} 字节", route.target, data.len()),
                }
            }
            _ => {
                log::debug!(
                    "字节路由忽略: {} -> {}.{} (端口域或节点类型不匹配)",
                    source_id,
                    route.target,
                    route.target_handle
                );
            }
        }
    }
}

/// 喂入 Protocol 节点: 解析 → 帧分发 → 旁路缓冲 → convert 链下推
///
/// 并行解析 (feed_parallel) 保留: 积压高时按帧边界切分并行, 积压低走顺序路径;
/// ParallelFeeder 按 Protocol 节点持有 (tokio mutex 跨 await)。
#[allow(clippy::too_many_arguments)]
async fn feed_protocol(
    plane: &DataPlaneState,
    app: Option<&AppHandle>,
    proto_id: &str,
    data: &[u8],
    depth_hint: usize,
    dec_cache: &mut DecoderFeedCache,
    summary: &mut RouteSummary,
    depth: usize,
) {
    let Some(st) = plane.protocol_states.lock().get(proto_id).cloned() else {
        log::debug!("协议节点无运行时状态, 跳过喂入: {}", proto_id);
        return;
    };
    let (engine, parallel) = {
        let s = st.lock();
        (s.engine.clone(), s.parallel.clone())
    };

    let cfg = *plane.pipeline_config.read();
    let workers = workers_needed(depth_hint, data.len(), &cfg);
    // 并行支持探测 (一次性): 帧定界协议 split_aligned 返回 Some
    let can_parallel = workers > 1 && {
        let mut s = st.lock();
        *s.parallel_supported
            .get_or_insert_with(|| engine.lock().split_aligned(&[], 2).is_some())
    };

    let out;
    let mut detection = None;
    if can_parallel {
        let mut par = parallel.lock().await;
        {
            let mut s = st.lock();
            if !s.in_parallel {
                // 首次进入并行: 接续主引擎内部缓冲里的半个帧
                s.in_parallel = true;
                par.pending = engine.lock().take_pending();
            }
        }
        let (o, det, _timing) = par.feed(&engine, data, workers).await;
        out = o;
        detection = det;
    } else {
        // 积压消退回落顺序模式: 不完整尾字节喂回主引擎 (零丢失)
        let was_parallel = {
            let mut s = st.lock();
            std::mem::replace(&mut s.in_parallel, false)
        };
        if was_parallel {
            let pending = parallel.lock().await.take_pending();
            if !pending.is_empty() {
                let _ = engine.lock().feed(&pending);
            }
        }
        let o = {
            let mut p = engine.lock();
            let o = p.feed(data);
            // 自动通道检测 (一次性), 与顺序路径共用同一锁 guard
            let notified = st.lock().detection_notified;
            if !notified && p.is_auto_mode() {
                detection = p.detected_channels();
            }
            o
        };
        out = o;
    }
    if detection.is_some() {
        st.lock().detection_notified = true;
    }
    if let (Some(app), Some(n)) = (app, detection) {
        crate::notify::channels_detected(app, n);
    }

    // CAN 帧旁路 (slcan/candleLight) — 全局缓冲 + 负载统计 (仅 Rx 计入)
    if !out.can_frames.is_empty() {
        let mut buf = plane.can_buffer.lock();
        let mut stats = plane.can_load_stats.lock();
        for f in out.can_frames {
            if f.direction == vofa_next_core::CanDirection::Rx {
                stats.push(&f);
            }
            buf.push(f);
        }
    }
    // 逻辑采样 / 解码事件旁路 — 全局缓冲
    if !out.logic_samples.is_empty() {
        let mut lb = plane.logic_buffer.lock();
        for s in out.logic_samples {
            lb.push(s);
        }
    }
    if !out.decoded_events.is_empty() {
        let mut db = plane.decoded_buffer.lock();
        for e in out.decoded_events {
            db.push(e);
        }
    }

    // 数据帧 → source_frames 缓存 + 触发数值平面评估
    if !out.frames.is_empty() {
        summary.frames += out.frames.len() as u64;
        summary.eval_ns += super::frame_dispatch::on_frames(plane, proto_id, &out.frames);
    }

    // convert_to: 输出引擎重编码 → 沿本节点 out 边继续下推 (协议转换链)
    let convert_engine = st.lock().convert_engine.clone();
    if let Some(ce) = convert_engine {
        let mut bytes = Vec::new();
        for f in &out.frames {
            bytes.extend_from_slice(&ce.lock().encode_frame(f));
        }
        if !bytes.is_empty() {
            Box::pin(route_inner(
                plane, app, proto_id, &bytes, 0, dec_cache, summary, depth + 1,
            ))
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use vofa_next_buffer::graph::Edge;
    use vofa_next_core::{ProtocolConfig, TransportConfig};
    use vofa_next_nodes::{BytePlan, DecoderBlockDef, FieldType, NodeDef};

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
        let summary =
            route_bytes(&plane, None, "tp", &firewater_bytes(&[1.0, 2.0, 3.0]), 0, &mut cache)
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
                    },
                ),
                node(
                    "pb",
                    NodeKind::Protocol {
                        config: ProtocolConfig::JustFloat { channels: Some(2) },
                        convert_to: None,
                    },
                ),
            ],
            vec![edge("tp", "rx", "pa", "in"), edge("pa", "out", "pb", "in")],
        );
        let mut cache = DecoderFeedCache::new();
        let summary =
            route_bytes(&plane, None, "tp", &firewater_bytes(&[4.0, 5.0]), 0, &mut cache).await;
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
        let graph = vofa_next_nodes::CompiledGraph::compile(
            "t1".into(),
            vec![u8_decoder("dec")],
            vec![],
        )
        .unwrap();
        plane.eval.graphs.lock().insert("t1".into(), graph);
        plane
            .eval
            .graphs_version
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let mut cache = DecoderFeedCache::new();
        let summary =
            route_bytes(&plane, None, "cmd", &firewater_bytes(&[7.0, 8.0]), 0, &mut cache).await;
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
}
