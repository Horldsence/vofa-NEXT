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
use node_kind::{
    NodeKind, FRAME_DECODER_IN_HANDLE, LOOPBACK_IN_HANDLE, PROTOCOL_IN_HANDLE, TRANSPORT_TX_HANDLE,
};

use super::DataPlaneState;
use crate::decoder_feed::DecoderFeedCache;
use crate::feed_parallel::workers_needed;

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
    route_inner(
        plane,
        app,
        source_id,
        data,
        depth_hint,
        dec_cache,
        &mut summary,
        0,
    )
    .await;
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
                feed_protocol(
                    plane,
                    app,
                    &route.target,
                    data,
                    depth_hint,
                    dec_cache,
                    summary,
                    depth,
                )
                .await;
            }
            (NodeKind::FrameDecoder { .. }, FRAME_DECODER_IN_HANDLE | LOOPBACK_IN_HANDLE) => {
                let ts = vofa_core::now_us();
                if crate::decoder_feed::feed_decoder_by_id(
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
                    Err(_) => log::warn!(
                        "传输注册表锁忙, 丢弃发往 {} 的 {} 字节",
                        route.target,
                        data.len()
                    ),
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
        log::debug!("协议节点无运行时状态, 跳过喂入: {proto_id}");
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
        notify_events::notify::channels_detected(app, n);
    }

    // CAN 帧旁路 (slcan/candleLight) — 全局缓冲 + 负载统计 (仅 Rx 计入)
    if !out.can_frames.is_empty() {
        let mut buf = plane.can_buffer.lock();
        let mut stats = plane.can_load_stats.lock();
        for f in out.can_frames {
            if f.direction == can_types::CanDirection::Rx {
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
                plane,
                app,
                proto_id,
                &bytes,
                0,
                dec_cache,
                summary,
                depth + 1,
            ))
            .await;
        }
    }
}
