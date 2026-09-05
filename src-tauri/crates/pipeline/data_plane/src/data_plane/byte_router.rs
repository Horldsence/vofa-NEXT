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

use kind::{
    NodeKind, FRAME_DECODER_IN_HANDLE, LOOPBACK_IN_HANDLE, PROTOCOL_IN_HANDLE, TRANSPORT_TX_HANDLE,
};
use tauri::AppHandle;

use super::protocol_feed::feed_protocol;
use super::DataPlaneState;
use crate::decoder_feed::DecoderFeedCache;

/// convert 链递归深度上限 (BytePlan 已保证 DAG, 此为防御性兜底)
const MAX_ROUTE_DEPTH: usize = 16;

/// 路由结果摘要 (统计 + 触发决策)
#[derive(Default, Debug, Clone)]
pub struct RouteSummary {
    /// 本次路由解析出的数据帧总数 (所有命中 Protocol 节点合计)
    pub frames: u64,
    /// 是否有 FrameDecoder 被喂入 (调用方据此做快照评估)
    pub decoders_fed: bool,
    /// Transport.tx 实际写入成功次数 (发送落地统计; 0 = 未命中任何 tx 边)
    pub tx_sends: u32,
    /// Transport.tx 写入失败/锁忙次数 — 统一发送内核据此向调用方报错
    pub tx_errors: u32,
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
    live: Option<&transport_core::LiveNodeHandle>,
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
        live,
    )
    .await;
    summary
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn route_inner(
    plane: &DataPlaneState,
    app: Option<&AppHandle>,
    source_id: &str,
    data: &[u8],
    depth_hint: usize,
    dec_cache: &mut DecoderFeedCache,
    summary: &mut RouteSummary,
    depth: usize,
    live: Option<&transport_core::LiveNodeHandle>,
) {
    if depth > MAX_ROUTE_DEPTH {
        log::warn!("字节路由深度超限 ({}), 丢弃 {} 字节", source_id, data.len());
        return;
    }
    // 路由表快照 (锁即刻释放, 下游分发不持 byte_plan 锁)
    let routes: Vec<_> = plane.byte_plan.lock().routes_for(source_id).to_vec();
    // 路由去重组 (不变量 4): 同 (字节源, 协议配置等价) 的 Protocol 目标只解析
    // 一次, 帧经 Arc fan-out 到组内各节点。组缺失时 (尚未 sync) 退回逐目标。
    let groups = plane.route_groups.lock().get(source_id).cloned();
    if let Some(groups) = groups.filter(|g| !g.is_empty()) {
        let grouped: std::collections::HashSet<&str> = groups
            .iter()
            .flat_map(|(_, _, members)| members.iter().map(String::as_str))
            .collect();
        // 非协议目标照常逐边分发
        for route in &routes {
            if grouped.contains(route.target.as_str()) {
                continue;
            }
            dispatch_non_protocol(
                plane,
                source_id,
                data,
                dec_cache,
                summary,
                &route.target,
                &route.target_handle,
                live,
            )
            .await;
        }
        // 每组只喂代表节点一次, 组员 fan-out
        for (_, repr, members) in &groups {
            feed_protocol(
                plane, app, source_id, repr, members, data, depth_hint, dec_cache, summary, depth,
                live,
            )
            .await;
        }
        return;
    }
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
                    source_id,
                    &route.target,
                    std::slice::from_ref(&route.target),
                    data,
                    depth_hint,
                    dec_cache,
                    summary,
                    depth,
                    live,
                )
                .await;
            }
            _ => {
                dispatch_non_protocol(
                    plane,
                    source_id,
                    data,
                    dec_cache,
                    summary,
                    &route.target,
                    &route.target_handle,
                    live,
                )
                .await;
            }
        }
    }
}

/// 非协议字节目标分发 — FrameDecoder 喂入 / Transport 回注发送 / 忽略日志
#[allow(clippy::too_many_arguments)]
async fn dispatch_non_protocol(
    plane: &DataPlaneState,
    source_id: &str,
    data: &[u8],
    dec_cache: &mut DecoderFeedCache,
    summary: &mut RouteSummary,
    target: &str,
    target_handle: &str,
    _live: Option<&transport_core::LiveNodeHandle>,
) {
    let kind = plane
        .global_nodes
        .lock()
        .get(target)
        .map(|n| n.kind.clone());
    let Some(kind) = kind else { return };
    match (&kind, target_handle) {
        (NodeKind::FrameDecoder { .. }, FRAME_DECODER_IN_HANDLE | LOOPBACK_IN_HANDLE) => {
            let ts = vofa_core::now_us();
            if crate::decoder_feed::feed_decoder_by_id(&plane.eval, target, data, ts, dec_cache) {
                summary.decoders_fed = true;
            }
        }
        (NodeKind::Transport { .. }, TRANSPORT_TX_HANDLE) => {
            // 协议转换回注 / 命令发送落地 — try_lock 避免与 open 的长持锁互等
            match plane.transport.try_lock() {
                Ok(m) => match m.send(target, data) {
                    Ok(()) => summary.tx_sends += 1,
                    Err(e) => {
                        summary.tx_errors += 1;
                        log::debug!("字节路由发送失败 ({target}): {e}");
                    }
                },
                Err(_) => {
                    summary.tx_errors += 1;
                    log::warn!("传输注册表锁忙, 丢弃发往 {} 的 {} 字节", target, data.len());
                }
            }
        }
        _ => {
            log::debug!(
                "字节路由忽略: {source_id} -> {target}.{target_handle} (端口域或节点类型不匹配)"
            );
        }
    }
}
