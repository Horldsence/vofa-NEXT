use app_state::{
    AppState, CustomInputBatch, GraphOutputSnapshot, SpectrumBatch, StringOutputSnapshot,
};
use buffer_graph::Edge;
use error::ConfigError;
use node_engine::BytePlan;
use node_kind::NodeDef;
use notify_events::emit_graph_derived;
use pipeline_data_plane::DataPlaneState;
use pipeline_data_plane::data_plane::{byte_router, frame_dispatch};
use pipeline_data_plane::decoder_feed::{sync_decoders_now, DecoderFeedCache};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{ipc::Channel, AppHandle, Emitter, State};
use vofa_core::{Error, Result};

use crate::{
    compile_queue, compute_derived, inject_protocol_sources, CompileState, GraphCompileEvent,
    GraphDerived,
};

// 全局队列入口保留 (供后续 LWW 后台 worker 接入, 当前同步实现不直接调用)
#[allow(unused_imports)]
use compile_queue::global as _global_queue;

// ============ 节点图 (后端化重构) ============

/// 用候选全局节点表 + 全部 tab 的字节边重建全局 BytePlan
///
/// 简单合并策略: 全局节点表按 id 覆盖合并 (任何 tab 提交后重建全局平面);
/// 孤儿节点 (图删除后残留) 的运行时资源由 `DataPlaneState::reconcile` 清理。
fn rebuild_byte_plan(
    graphs: &Arc<parking_lot::Mutex<HashMap<String, node_engine::CompiledGraph>>>,
    candidate: &std::collections::HashMap<String, NodeDef>,
    new_tab: Option<(&str, &node_engine::CompiledGraph)>,
) -> Result<BytePlan> {
    let mut byte_edges: Vec<Edge> = Vec::new();
    {
        let graphs = graphs.lock();
        for (tab_id, g) in graphs.iter() {
            if new_tab.is_some_and(|(id, _)| id == tab_id) {
                continue; // 本 tab 用新图的边
            }
            byte_edges.extend(g.byte_edges());
        }
    }
    if let Some((_, g)) = new_tab {
        byte_edges.extend(g.byte_edges());
    }
    // 合并后的全局表先建 HIR (边分类应与各 tab 编译期一致), 再投影字节平面
    let typed = node_engine::TypedGraph::build(candidate.values().cloned(), byte_edges)
        .map_err(|e| Error::Config(ConfigError::BytePlanCompile(Box::new(e))))?;
    BytePlan::build(&typed).map_err(|e| Error::Config(ConfigError::BytePlanCompile(Box::new(e))))
}

/// 更新指定 tab 的节点图 (整体替换 nodes + edges)
///
/// 两层编译:
/// 1. 本 tab 数值图 CompiledGraph::compile (f32 槽位 + 本 tab BytePlan)
/// 2. 全局字节平面: 该 tab 节点按 id 覆盖合并进全局节点表, 所有 tab 的
///    字节边合并重算全局 BytePlan 存入 DataPlaneState, 并同步 protocol_states
///
/// 任一层编译失败 (循环/端口域不匹配等) 返回错误, 旧图与旧平面保留。
/// 提交成功后返回 [`GraphDerived`] (本次图变化涉及的全部节点派生端口表 / 通道数),
/// 同时 emit `graph:derived` 事件给前端。
#[tauri::command]
pub async fn update_tab_graph(
    state: State<'_, AppState>,
    app: AppHandle,
    tab_id: String,
    nodes: Vec<NodeDef>,
    edges: Vec<Edge>,
) -> Result<GraphDerived> {
    apply_tab_graph(&state, Some(&app), tab_id, nodes, edges).await
}

/// `update_tab_graph` 的实现本体 (抽出以便不依赖 Tauri State 地测试)
///
/// `app`: Tauri AppHandle, 用于 emit `graph:derived` 与 `graph:compile` 事件; 测试时可传 None
pub async fn apply_tab_graph(
    state: &AppState,
    app: Option<&AppHandle>,
    tab_id: String,
    nodes: Vec<NodeDef>,
    edges: Vec<Edge>,
) -> Result<GraphDerived> {
    apply_tab_graph_parts(
        &state.graphs,
        &state.graphs_version,
        &state.data_plane,
        app,
        tab_id,
        nodes,
        edges,
    )
    .await
}

/// [`apply_tab_graph`] 的部件版 — 只依赖图状态三件套
/// (tab 图表 / 全局版本号 / 数据平面), 供 MCP server 等非 Tauri-State
/// 场景直接复用同一条提交路径。
// 参数类型与 AppState.graphs 字段完全一致 (std hasher), 不做 hasher 泛型化
#[allow(clippy::implicit_hasher)]
pub async fn apply_tab_graph_parts(
    graphs: &Arc<parking_lot::Mutex<HashMap<String, node_engine::CompiledGraph>>>,
    graphs_version: &Arc<std::sync::atomic::AtomicU64>,
    data_plane: &DataPlaneState,
    app: Option<&AppHandle>,
    tab_id: String,
    nodes: Vec<NodeDef>,
    edges: Vec<Edge>,
) -> Result<GraphDerived> {
    // 1. ProtocolSource 自动注入 (后端单一权威 — 前端不再下发 ProtocolSource NodeDef)
    let mut compile_nodes = nodes.clone();
    compile_nodes.extend(inject_protocol_sources(&nodes, &edges));

    // 2. 本 tab 数值图编译 — 失败时构造 `CompileReport` 并 emit `graph:compile` 事件
    let compiled = match node_engine::CompiledGraph::compile(tab_id.clone(), compile_nodes, edges) {
        Ok(g) => g,
        Err(e) => {
            let report = error::CompileReport::new(e);
            if let Some(app) = app {
                let _ = app.emit(
                    crate::GRAPH_COMPILE_EVENT,
                    GraphCompileEvent {
                        tab_id: tab_id.clone(),
                        state: CompileState::Error,
                        queued_seq: 0,
                        report: Some(report),
                    },
                );
            }
            return Err(Error::Config(ConfigError::GraphCompile(Box::new(
                error::CompileError::Cycle { cycle: vec![] },
            ))));
        }
    };

    // 3. 候选全局节点表: 移除该 tab 旧节点 → 插入新节点 (按 id 覆盖)
    // ProtocolSource 是 tab 数值平面的帧源引用, 不参与字节平面, 不进全局表
    // (避免与全局 Protocol 定义同 id 冲突)
    let mut candidate = data_plane.global_nodes.lock().clone();
    candidate.retain(|_, n| n.tab_id != tab_id);
    for n in &nodes {
        if matches!(n.kind, node_kind::NodeKind::ProtocolSource { .. }) {
            continue;
        }
        candidate.insert(n.id.clone(), n.clone());
    }

    // 4. 全局字节平面重建 (失败则不提交任何状态)
    let plan = rebuild_byte_plan(graphs, &candidate, Some((&tab_id, &compiled)))?;

    // 5. 派生数据计算 (本次图变化涉及的全部节点的输出端口表 / 通道数)
    let derived_nodes = compute_derived(&candidate.values().cloned().collect::<Vec<_>>());

    // 6. 提交: tab 图 + 全局节点表 + 全局平面 + 版本号
    graphs.lock().insert(tab_id.clone(), compiled);
    *data_plane.global_nodes.lock() = candidate;
    *data_plane.byte_plan.lock() = plan;
    graphs_version.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    // 7. 同步 Protocol 节点运行时状态 + FrameDecoder 状态清理 + 孤儿资源清理
    data_plane.sync_protocol_states();
    data_plane.reconcile().await;
    sync_decoders_now(&data_plane.eval.clone());

    // 8. 立即快照评估一次: 图结构/参数变更必须即时反映到输出,
    //    不能依赖 transport 数据流 — 无数据流时 manual Trigger 改 command、
    //    Str 节点内联框编辑也要立即出结果 (同 set_input_value 语义)
    frame_dispatch::refresh_snapshot(data_plane);

    let derived = GraphDerived {
        nodes: derived_nodes,
    };
    if let Some(app) = app {
        emit_graph_derived(app, &derived);
        let _ = app.emit(
            crate::GRAPH_COMPILE_EVENT,
            GraphCompileEvent {
                tab_id: tab_id.clone(),
                state: CompileState::Ok,
                queued_seq: 0,
                report: None,
            },
        );
    }
    Ok(derived)
}

/// 移除指定 tab 的节点图 (tab 删除时调用)
#[tauri::command]
pub async fn remove_tab_graph(
    state: State<'_, AppState>,
    app: AppHandle,
    tab_id: String,
) -> Result<GraphDerived> {
    apply_remove_tab_graph(&state, Some(&app), &tab_id).await
}

/// `remove_tab_graph` 的实现本体 (抽出以便不依赖 Tauri State 地测试)
pub async fn apply_remove_tab_graph(
    state: &AppState,
    app: Option<&AppHandle>,
    tab_id: &str,
) -> Result<GraphDerived> {
    state.graphs.lock().remove(tab_id);

    // 全局节点表移除该 tab 节点 + 重建全局字节平面
    let mut candidate = state.data_plane.global_nodes.lock().clone();
    candidate.retain(|_, n| n.tab_id != tab_id);
    // 在移动 candidate 前计算派生数据 (后置消费者仍需遍历)
    let derived_nodes = compute_derived(&candidate.values().cloned().collect::<Vec<_>>());
    let plan = rebuild_byte_plan(&state.graphs, &candidate, None)?;
    *state.data_plane.global_nodes.lock() = candidate;
    *state.data_plane.byte_plan.lock() = plan;
    state
        .graphs_version
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    state.data_plane.sync_protocol_states();
    state.data_plane.reconcile().await;
    sync_decoders_now(&state.eval_state());

    // 立即快照评估一次 (同 update_tab_graph): 被删 tab 节点的输出键
    // 随全量覆盖写立即从快照清除, 不依赖 transport 数据流
    frame_dispatch::refresh_snapshot(&state.data_plane);

    let derived = GraphDerived {
        nodes: derived_nodes,
    };
    if let Some(app) = app {
        emit_graph_derived(app, &derived);
    }
    Ok(derived)
}

/// 设置输入控件当前值 (Knob/Slider/Button/Radio/Checkbox 拖动时调用)
///
/// 该值会在下一帧 evaluate 时作为 Input 节点的输出
///
/// 立即快照评估一次: 输入控件的值变化必须即时反映到图输出,
/// 不能依赖 transport 数据流 — 断开/无帧时下游 (CommandSender onChange 发送、
/// Gauge 等显示控件) 也要能感知变化。
#[tauri::command]
pub async fn set_input_value(
    state: State<'_, AppState>,
    widget_id: String,
    value: f32,
) -> Result<()> {
    state.input_values.lock().insert(widget_id, value);
    frame_dispatch::refresh_snapshot(&state.data_plane);
    Ok(())
}

/// 提交 Custom widget 的输出 (前端 iframe 调用 ctx.send 后回传)
///
/// 后端在下一帧 evaluate 时使用这些值作为 Custom 节点的输出
/// (同 set_input_value: 立即快照评估, 不依赖 transport 数据流)
#[tauri::command]
pub async fn submit_custom_output(
    state: State<'_, AppState>,
    widget_id: String,
    outputs: std::collections::HashMap<String, f32>,
) -> Result<()> {
    state.custom_outputs.lock().insert(widget_id, outputs);
    frame_dispatch::refresh_snapshot(&state.data_plane);
    Ok(())
}

/// 提交字符串输出 — 保留给 Custom JS widget 的字符串输出回传通道
///
/// (Trigger 的字符串规则已由后端图求值直接产出, 不再走此命令;
///  当前前端尚无调用方)
///
/// 写入 `custom_text_outputs` map; 后端 `text_output_ticker` 自适应速率推送给
/// 订阅了 `subscribe_string_outputs` 的前端 (TextDisplay 控件读取显示)
#[tauri::command]
pub async fn submit_custom_text_output(
    state: State<'_, AppState>,
    widget_id: String,
    outputs: std::collections::HashMap<String, String>,
) -> Result<()> {
    state.custom_text_outputs.lock().insert(widget_id, outputs);
    Ok(())
}

/// 字节注入 — CommandSender 回环模式 / 协议调试的发送路径
/// (取代旧 inject_loopback_bytes: loopback 字符串特判 → 全局 BytePlan 路由)
///
/// 将字节沿全局 BytePlan 中 `source_node_id` 的下游字节边路由:
/// - FrameDecoder.in: 喂入解析 (与实时 RX 同等对待: 更新 last_frame + 旁路收集)
/// - Protocol.in: 喂入协议引擎 (产帧进 source_frames + 触发数值平面)
/// - Transport.tx: 经传输注册表发送 (回注落地)
///
/// 与串口开关无关 — 无连接时也能工作 (路由不依赖 transport 状态)。
///
/// 返回: 路由命中的下游数量 (0 = 未连线, 前端可忽略)
#[tauri::command]
pub async fn inject_bytes(
    app: AppHandle,
    state: State<'_, AppState>,
    source_node_id: String,
    data: Vec<u8>,
) -> Result<usize> {
    let plane = state.data_plane.clone();
    let target_count = plane.byte_plan.lock().routes_for(&source_node_id).len();

    let mut cache = DecoderFeedCache::new();
    let summary =
        byte_router::route_bytes(&plane, Some(&app), &source_node_id, &data, 0, &mut cache).await;

    // FrameDecoder 被喂入 → 快照评估一次 (decoder 输出来自 last_frame 缓存)
    if summary.decoders_fed {
        frame_dispatch::refresh_snapshot(&plane);
    }

    Ok(target_count)
}

/// 订阅图输出快照 — 60 FPS 推送 HashMap<widgetId, HashMap<portId, value>>
///
/// 前端通过单一订阅获取所有节点的实时输出值
#[tauri::command]
pub async fn subscribe_graph_outputs(
    state: State<'_, AppState>,
    on_event: Channel<GraphOutputSnapshot>,
) -> Result<()> {
    state.output_subscribers.lock().push(on_event);
    Ok(())
}

/// 订阅 Custom widget 输入批次 — 30 FPS 推送
///
/// 前端收到后转发到对应 iframe
#[tauri::command]
pub async fn subscribe_custom_inputs(
    state: State<'_, AppState>,
    on_event: Channel<CustomInputBatch>,
) -> Result<()> {
    state.custom_input_subscribers.lock().push(on_event);
    Ok(())
}

/// 订阅字符串输出快照 — 与 graph_outputs 平行的字符串平面
///
/// TextDisplay 控件通过此订阅获取所有触发器的最新字符串输出
#[tauri::command]
pub async fn subscribe_string_outputs(
    state: State<'_, AppState>,
    on_event: Channel<StringOutputSnapshot>,
) -> Result<()> {
    state.text_output_subscribers.lock().push(on_event);
    Ok(())
}

/// 取消字符串输出订阅
#[tauri::command]
pub async fn unsubscribe_string_outputs(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    let mut subs = state.text_output_subscribers.lock();
    subs.retain(|_ch| {
        // Channel 内部 id 通过 ChannelId 获取 — 这里按引用相等不可行,
        // 由前端在 unmount 时清空所有 subscribers 即可。
        // 此命令主要给前端对称调用, 实际清理在前端 closeTauriChannel 中完成。
        let _ = channel_id;
        true
    });
    Ok(())
}

/// 订阅频谱分析结果 — 30 FPS 推送 SpectrumBatch
///
/// 前端 SpectrumChart 通过此订阅获取所有 SpectrumSink 节点的最新 FFT 结果。
/// batch.spectra: HashMap<sinkWidgetId, SpectrumResult>
/// 即使某 sink 的窗口未填满 (尚未产生新结果), 也会推送 snapshot 中的上一帧值,
/// 保证新订阅者立即收到数据, 图表连续不闪烁。
#[tauri::command]
pub async fn subscribe_spectrum(
    state: State<'_, AppState>,
    on_event: Channel<SpectrumBatch>,
) -> Result<()> {
    state.spectrum_subscribers.lock().push(on_event);
    Ok(())
}

/// 取消订阅图输出 — 从订阅者列表中移除指定 channel
///
/// 前端在取消订阅时应先调用此命令移除后端引用, 再注销 JS 端回调,
/// 避免后端向已关闭的 channel 发送数据时产生 "Couldn't find callback id" 警告。
#[tauri::command]
pub async fn unsubscribe_graph_outputs(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    let mut subs = state.output_subscribers.lock();
    subs.retain(|ch| ch.id() != channel_id);
    Ok(())
}

/// 取消订阅 Custom 输入 — 从订阅者列表中移除指定 channel
#[tauri::command]
pub async fn unsubscribe_custom_inputs(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    let mut subs = state.custom_input_subscribers.lock();
    subs.retain(|ch| ch.id() != channel_id);
    Ok(())
}

/// 取消订阅频谱 — 从订阅者列表中移除指定 channel
#[tauri::command]
pub async fn unsubscribe_spectrum(state: State<'_, AppState>, channel_id: u32) -> Result<()> {
    let mut subs = state.spectrum_subscribers.lock();
    subs.retain(|ch| ch.id() != channel_id);
    Ok(())
}

// ============ 测试 ============

#[cfg(test)]
mod tests {
    use super::*;
    use node_kind::NodeKind;

    fn input_node(id: &str, tab_id: &str) -> NodeDef {
        NodeDef {
            id: id.into(),
            tab_id: tab_id.into(),
            kind: NodeKind::Input,
        }
    }

    /// update_tab_graph 提交后必须立即快照评估: 无 transport 数据流时,
    /// 图/参数变更 (manual Trigger 改 command、Str 内联框编辑等) 也要即时
    /// 反映到 output_snapshot (回归: 曾缺 refresh_snapshot 调用)
    #[tokio::test]
    async fn update_tab_graph_refreshes_snapshot() {
        let state = AppState::new();
        state.input_values.lock().insert("in1".into(), 7.0);

        apply_tab_graph(
            &state,
            None,
            "tab1".into(),
            vec![input_node("in1", "tab1")],
            vec![],
        )
        .await
        .expect("提交图应成功");

        let got = state
            .data_plane
            .eval
            .output_snapshot
            .lock()
            .values
            .get("in1")
            .and_then(|ports| ports.get("value"))
            .copied();
        assert_eq!(got, Some(7.0), "提交后应立即快照评估, Input 值立即可见");
    }

    /// remove_tab_graph 提交后同样立即快照评估: 快照为全量覆盖写,
    /// 被删 tab 的节点输出键应立即从快照清除
    #[tokio::test]
    async fn remove_tab_graph_refreshes_snapshot() {
        let state = AppState::new();
        state.input_values.lock().insert("in1".into(), 3.0);
        apply_tab_graph(
            &state,
            None,
            "tab1".into(),
            vec![input_node("in1", "tab1")],
            vec![],
        )
        .await
        .expect("提交图应成功");
        assert!(
            state
                .data_plane
                .eval
                .output_snapshot
                .lock()
                .values
                .contains_key("in1"),
            "前提: 提交后输出已可见"
        );

        apply_remove_tab_graph(&state, None, "tab1")
            .await
            .expect("移除图应成功");

        let cleared = !state
            .data_plane
            .eval
            .output_snapshot
            .lock()
            .values
            .contains_key("in1");
        assert!(cleared, "移除后应立即快照评估, 过期节点键立即清除");
    }
}
