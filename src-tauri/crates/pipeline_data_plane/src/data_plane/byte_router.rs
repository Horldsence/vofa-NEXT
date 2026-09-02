//! 字节路由 — 沿全局 BytePlan 把字节事件推送到所有下游
//!
//! 入口 [`route_bytes`]: 以 `source_id` (Transport 节点 / widget loopbackOut /
//! Protocol 节点 convert 链) 为源, 查 `BytePlan::routes_for` 逐个下游分发:
//! - Protocol 节点 `in`: 喂入解析引擎 (保留合批后的顺序/并行解析),
//!   产帧 → [`super::frame_dispatch::on_frames`] 写 source_frames + 触发数值平面;
//!   can/logic/decoded 旁路进全局缓冲; 若有 convert_to, 输出引擎 encode_frame
//!   重编码 → 沿本节点 `out` 边递归下推 (BytePlan 拓扑序保证无环, 另有深度上限兜底);
//!   RawData 协议不产帧: 原始字节 UTF-8 lossy 解码缓存到 source_texts
//!   (ProtocolSource "str" 端口数据源), 无 convert_to 时原始字节沿 `out` 边透传下推
//! - FrameDecoder 节点 `in`/`loopbackIn`: 走 feed_one_decoder 语义 (按边路由)
//! - Transport 节点 `tx`: registry.send (协议转换回注 / 命令发送落地)

use node_kind::{
    NodeKind, FRAME_DECODER_IN_HANDLE, LOOPBACK_IN_HANDLE, PROTOCOL_IN_HANDLE, TRANSPORT_TX_HANDLE,
};
use schema_types::{ProtocolConfig, SchemaPreset};
use tauri::AppHandle;

use super::DataPlaneState;
use crate::decoder_feed::DecoderFeedCache;
use crate::feed_parallel::workers_needed;

/// convert 链递归深度上限 (BytePlan 已保证 DAG, 此为防御性兜底)
const MAX_ROUTE_DEPTH: usize = 16;

/// 通道检测推送判定: 当前检测值与上次已推送值不同则返回本次应推送的通道数
/// (None = 尚未检测到或与上次同值, 不推; 首次检测到 None→Some(n) 视为变化)
fn channels_detection_change(last_pushed: Option<usize>, current: Option<usize>) -> Option<usize> {
    match current {
        Some(n) if last_pushed != Some(n) => Some(n),
        _ => None,
    }
}

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
/// - `app`: 自动通道检测的系统通知与 `protocol:channels-detected` 事件推送用
///   (测试/无界面路径传 None, 跳过 emit 但 buffer 通道数对齐仍生效)
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

/// 喂入 Protocol 节点: 解析 → 帧分发 → 旁路缓冲 → convert 链下推 / RawData 文本缓存+透传
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

    let mut detection = None;
    let mut out = if can_parallel {
        // 首次进入并行: 接续主引擎内部缓冲里的半个帧 (false→true 转换沿)
        let enter_parallel = {
            let mut s = st.lock();
            !std::mem::replace(&mut s.in_parallel, true)
        };
        let mut par = parallel.lock().await;
        if enter_parallel {
            par.pending = engine.lock().take_pending();
        }
        let (o, det, _timing) = par.feed(&engine, data, workers).await;
        detection = det;
        o
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
        {
            let mut p = engine.lock();
            let o = p.feed(data);
            // 自动通道检测: 自动模式下每次读取检测值, 变化即推 (见下方检测值处理)
            if p.is_auto_mode() {
                detection = p.detected_channels();
            }
            o
        }
    };
    // 通道检测处理 (单次锁内取齐决策):
    // - 系统通知保持一次性语义 (detection_notified 闸)
    // - 前端事件 protocol:channels-detected 按变化推送 (last_detected_pushed 记录上次已推送值),
    //   同一点位把该源 buffer 通道数对齐到检测值 (自动模式下 config.channels 必为 None,
    //   effective 即 detected)
    let (notify_once, push) = {
        let mut s = st.lock();
        let notify_once = if detection.is_some() && !s.detection_notified {
            s.detection_notified = true;
            detection
        } else {
            None
        };
        let push = channels_detection_change(s.last_detected_pushed, detection);
        if push.is_some() {
            s.last_detected_pushed = push;
        }
        drop(s);
        (notify_once, push)
    };
    if let (Some(app), Some(n)) = (app, notify_once) {
        notify_events::notify::channels_detected(app, n);
    }
    if let Some(n) = push {
        if let Some(app) = app {
            notify_events::emit_protocol_channels_detected(app, proto_id, n);
        }
        plane.buffer_for(proto_id).lock().set_channels(n);
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

    // RawData 判定 + convert 引擎 (一次锁取齐)
    // 有效预设判定: 有 schema 时按 preset (用户编辑块后 preset=Custom, 走 SchemaEngine 产帧,
    // 不再做文本缓存/原文透传); 无 schema (旧前端) 回退按 config.kind
    let (convert_engine, is_raw_data) = {
        let s = st.lock();
        (
            s.convert_engine.clone(),
            s.schema.as_ref().map_or_else(
                || matches!(s.config, ProtocolConfig::RawData),
                |schema| schema.preset == SchemaPreset::RawData,
            ),
        )
    };

    // RawData 协议不产帧: 原始字节写 source_texts 文本缓存
    // (ProtocolSource "str" 端口数据源, 正式入口见 frame_dispatch::cache_source_text)
    if is_raw_data {
        super::frame_dispatch::cache_source_text(plane, proto_id, data);
    }

    // convert_to: 输出引擎重编码 → 沿本节点 out 边继续下推 (协议转换链)
    // (提前计算: frames 随后按值移入 blocking 评估任务)
    let converted = match convert_engine {
        Some(ce) => {
            let mut bytes = Vec::new();
            for f in &out.frames {
                bytes.extend_from_slice(&ce.lock().encode_frame(f));
            }
            bytes
        }
        None => Vec::new(),
    };

    // 数据帧 → source_frames 缓存 + 触发数值平面评估
    // 重型同步评估 (大批次可达 24ms+) 丢 blocking 池执行, 不占 tokio worker;
    // await 保持同源批序, panic 语义透传 (只吞关闭型 JoinError)
    if !out.frames.is_empty() {
        summary.frames += out.frames.len() as u64;
        let eval = plane.eval.clone();
        let global_nodes = plane.global_nodes.clone();
        let buffer = plane.buffer_for(proto_id);
        let proto = proto_id.to_string();
        let frames = std::mem::take(&mut out.frames);
        let eval_workers = cfg.eval_workers;
        let eval_task = tokio::task::spawn_blocking(move || {
            super::frame_dispatch::on_frames_detached(
                &eval,
                &global_nodes,
                &buffer,
                &proto,
                &frames,
                eval_workers,
            )
        });
        match eval_task.await {
            Ok(ns) => summary.eval_ns += ns,
            Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
            Err(_) => {}
        }
    }

    if !converted.is_empty() {
        Box::pin(route_inner(
            plane,
            app,
            proto_id,
            &converted,
            0,
            dec_cache,
            summary,
            depth + 1,
        ))
        .await;
    } else if is_raw_data && !data.is_empty() {
        // RawData 不产帧 (无论是否设置 convert_to, 重编码产物恒为空):
        // 原始字节沿本节点 out 边透传下推 (可接 FrameDecoder / 其他 Transport.tx),
        // 避免设置 convert_to 后原文被静默丢弃
        Box::pin(route_inner(
            plane,
            app,
            proto_id,
            data,
            0,
            dec_cache,
            summary,
            depth + 1,
        ))
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 变化推送判定: None 不推; 同值不推; None→Some(n) 与 Some(a)→Some(b) 推
    #[test]
    fn channels_detection_change_semantics() {
        assert_eq!(channels_detection_change(None, None), None, "未检测不推");
        assert_eq!(
            channels_detection_change(Some(3), None),
            None,
            "本次未检测不推"
        );
        assert_eq!(
            channels_detection_change(None, Some(3)),
            Some(3),
            "首次检测即变化"
        );
        assert_eq!(
            channels_detection_change(Some(3), Some(3)),
            None,
            "同值不重复推"
        );
        assert_eq!(
            channels_detection_change(Some(3), Some(5)),
            Some(5),
            "检测值变化推新值"
        );
    }
}
