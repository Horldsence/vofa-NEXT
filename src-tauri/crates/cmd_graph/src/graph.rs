use app_state::{
    prune_positions, AppState, Position, SourceGraphs, SourceNodeHint, TabSourceGraph,
    WidgetRecord, WorkspaceState,
};
use buffer_graph::Edge;
use error::ConfigError;
use node_engine::BytePlan;
use node_kind::NodeDef;
use notify_events::emit_graph_derived;
use pipeline_data_plane::data_plane::{byte_router, frame_dispatch};
use pipeline_data_plane::decoder_feed::{sync_decoders_now, DecoderFeedCache};
use pipeline_data_plane::DataPlaneState;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use vofa_core::{Error, Result};

use crate::{
    compile_queue, compute_derived, inject_protocol_sources, CompileState, GraphCompileEvent,
    GraphDerived, GraphSourceEvent, GRAPH_SOURCE_EVENT,
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
/// 任一层编译失败 (循环/端口域不匹配等) 返回真实编译错误, 旧图与旧平面保留。
/// `widgets` / `positions` 为 widget 配置记录与画布位置 (配置模型的后端权威存储):
/// Some 时整体替换 / 合并, None (拓扑 op 等增量写入方) 时保留现状。
/// `base_version` 提供时做乐观并发检查: 与当前图版本不符返回
/// `GraphVersionConflict` (期间有其他写入方 — 拓扑 op / MCP — 推进了版本)。
/// 提交成功后返回 [`GraphDerived`] (派生端口表 + 新版本号),
/// 同时 emit `graph:derived` 与 `graph:source` (权威源图) 事件给前端。
#[allow(clippy::implicit_hasher)]
#[tauri::command]
pub async fn update_tab_graph(
    state: State<'_, AppState>,
    app: AppHandle,
    tab_id: String,
    nodes: Vec<NodeDef>,
    edges: Vec<Edge>,
    node_hints: Option<HashMap<String, SourceNodeHint>>,
    widgets: Option<Vec<WidgetRecord>>,
    positions: Option<HashMap<String, Position>>,
    base_version: Option<u64>,
) -> Result<GraphDerived> {
    apply_tab_graph(
        &state,
        Some(&app),
        tab_id,
        nodes,
        edges,
        node_hints.unwrap_or_default(),
        widgets,
        positions,
        base_version,
    )
    .await
}

/// `update_tab_graph` 的实现本体 (抽出以便不依赖 Tauri State 地测试)
///
/// `app`: Tauri AppHandle, 用于 emit `graph:derived` / `graph:compile` /
/// `graph:source` 事件; 测试时可传 None
#[allow(clippy::implicit_hasher)]
#[allow(clippy::too_many_arguments)]
pub async fn apply_tab_graph(
    state: &AppState,
    app: Option<&AppHandle>,
    tab_id: String,
    nodes: Vec<NodeDef>,
    edges: Vec<Edge>,
    node_hints: HashMap<String, SourceNodeHint>,
    widgets: Option<Vec<WidgetRecord>>,
    positions: Option<HashMap<String, Position>>,
    base_version: Option<u64>,
) -> Result<GraphDerived> {
    apply_tab_graph_parts(
        &state.graphs,
        &state.graphs_version,
        &state.data_plane,
        &state.source_graphs,
        &state.workspace,
        app,
        tab_id,
        nodes,
        edges,
        node_hints,
        widgets,
        positions,
        base_version,
    )
    .await
}

/// [`apply_tab_graph`] 的部件版 — 只依赖图状态五件套
/// (tab 图表 / 全局版本号 / 数据平面 / 源图存储 / 工作区), 供 MCP server、
/// 拓扑 op 等非 Tauri-State 场景直接复用同一条提交路径。
///
/// 成功后把 `(nodes, edges, hints, widgets)` 写入源图存储、合并 positions,
/// 并 emit `graph:source`; 编译失败所有存储不变。
// 参数类型与 AppState.graphs 字段完全一致 (std hasher), 不做 hasher 泛型化
#[allow(clippy::implicit_hasher)]
#[allow(clippy::too_many_arguments)]
pub async fn apply_tab_graph_parts(
    graphs: &Arc<parking_lot::Mutex<HashMap<String, node_engine::CompiledGraph>>>,
    graphs_version: &Arc<std::sync::atomic::AtomicU64>,
    data_plane: &DataPlaneState,
    source_graphs: &SourceGraphs,
    workspace: &WorkspaceState,
    app: Option<&AppHandle>,
    tab_id: String,
    nodes: Vec<NodeDef>,
    edges: Vec<Edge>,
    node_hints: HashMap<String, SourceNodeHint>,
    widgets: Option<Vec<WidgetRecord>>,
    positions: Option<HashMap<String, Position>>,
    base_version: Option<u64>,
) -> Result<GraphDerived> {
    // 0. 乐观并发检查 — base_version 过期说明期间有其他写入方推进了图,
    //    整图替换会覆盖掉那批变更, 必须拒绝 (前端据此拉取权威源图合并重试)
    if let Some(base) = base_version {
        let current = graphs_version.load(std::sync::atomic::Ordering::Relaxed);
        if current != base {
            return Err(Error::Config(ConfigError::GraphVersionConflict { current }));
        }
    }

    // 1. ProtocolSource 自动注入 (后端单一权威 — 前端不再下发 ProtocolSource NodeDef)
    let mut compile_nodes = nodes.clone();
    compile_nodes.extend(inject_protocol_sources(&nodes, &edges));

    // 2. 本 tab 数值图编译 — 失败时构造 `CompileReport` 并 emit `graph:compile` 事件,
    //    真实编译错误原样返回 (占位假错误会吞掉域不匹配等可用原因)
    let compiled =
        match node_engine::CompiledGraph::compile(tab_id.clone(), compile_nodes, edges.clone()) {
            Ok(g) => g,
            Err(e) => {
                let report = error::CompileReport::new(e.clone());
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
                return Err(Error::Config(ConfigError::GraphCompile(Box::new(e))));
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

    // 6. 提交: tab 图 + 全局节点表 + 全局平面 + 版本号 + 源图存储 + 工作区
    graphs.lock().insert(tab_id.clone(), compiled);
    *data_plane.global_nodes.lock() = candidate;
    *data_plane.byte_plan.lock() = plan;
    let version = graphs_version.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    // widget 记录: 提交携带时整体替换, 增量写入方 (拓扑 op / MCP 纯拓扑) 保留现状
    let stored_widgets = {
        let mut store = source_graphs.lock();
        let widgets = widgets.unwrap_or_else(|| {
            store
                .get(&tab_id)
                .map(|g| g.widgets.clone())
                .unwrap_or_default()
        });
        store.insert(
            tab_id.clone(),
            TabSourceGraph {
                nodes,
                edges,
                hints: node_hints,
                widgets: widgets.clone(),
            },
        );
        widgets
    };
    // 工作区: 画布位置合并 + 孤儿位置清理 (存活集合 = 全部 tab 源图节点)
    {
        let mut ws = workspace.lock();
        if let Some(pos) = positions {
            ws.positions.extend(pos);
        }
        ws.dirty = true;
    }
    prune_positions(workspace, source_graphs);

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
        version,
    };
    if let Some(app) = app {
        emit_graph_derived(app, &derived);
        // 权威源图回推 — 前端画布据此收敛 (多写入方: 前端提交 / 拓扑 op / MCP)。
        // 携带 widget 配置记录与画布位置: 画布按此重建该 tab 完整视图
        // (外部提交的纯 widget 图也可完整渲染)
        let (nodes, edges, widgets) = {
            let store = source_graphs.lock();
            let g = store.get(&tab_id);
            (
                g.map(|g| g.nodes.clone()).unwrap_or_default(),
                g.map(|g| g.edges.clone()).unwrap_or_default(),
                stored_widgets,
            )
        };
        let tab_node_ids: std::collections::HashSet<String> = source_graphs
            .lock()
            .get(&tab_id)
            .map(|g| g.nodes.iter().map(|n| n.id.clone()).collect())
            .unwrap_or_default();
        let event_positions: HashMap<String, Position> = workspace
            .lock()
            .positions
            .iter()
            .filter(|(id, _)| tab_node_ids.contains(*id))
            .map(|(id, p)| (id.clone(), *p))
            .collect();
        let _ = app.emit(
            GRAPH_SOURCE_EVENT,
            GraphSourceEvent {
                tab_id: tab_id.clone(),
                version,
                nodes,
                edges,
                widgets,
                positions: event_positions,
            },
        );
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
    // 源图存储同步清除 — tab 已不存在, 权威拓扑与 widget 记录随之失效
    state.source_graphs.lock().remove(tab_id);
    prune_positions(&state.workspace, &state.source_graphs);
    state.workspace.lock().dirty = true;

    // 全局节点表移除该 tab 节点 + 重建全局字节平面
    let mut candidate = state.data_plane.global_nodes.lock().clone();
    candidate.retain(|_, n| n.tab_id != tab_id);
    // 在移动 candidate 前计算派生数据 (后置消费者仍需遍历)
    let derived_nodes = compute_derived(&candidate.values().cloned().collect::<Vec<_>>());
    let plan = rebuild_byte_plan(&state.graphs, &candidate, None)?;
    *state.data_plane.global_nodes.lock() = candidate;
    *state.data_plane.byte_plan.lock() = plan;
    let version = state
        .graphs_version
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        + 1;

    state.data_plane.sync_protocol_states();
    state.data_plane.reconcile().await;
    sync_decoders_now(&state.eval_state());

    // 立即快照评估一次 (同 update_tab_graph): 被删 tab 节点的输出键
    // 随全量覆盖写立即从快照清除, 不依赖 transport 数据流
    frame_dispatch::refresh_snapshot(&state.data_plane);

    let derived = GraphDerived {
        nodes: derived_nodes,
        version,
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
    state.input_values.write().insert(widget_id, value);
    frame_dispatch::refresh_snapshot(&state.data_plane);
    Ok(())
}

/// 上报节点画布位置 (拖拽结束时批量提交) — 轻量路径, 不触发编译,
/// 仅更新工作区位置表并标记落盘脏
#[allow(clippy::implicit_hasher)]
#[tauri::command]
pub fn set_node_positions(
    state: State<'_, AppState>,
    positions: HashMap<String, Position>,
) -> Result<()> {
    let mut ws = state.workspace.lock();
    ws.positions.extend(positions);
    ws.dirty = true;
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
    state.custom_outputs.write().insert(widget_id, outputs);
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
        state.input_values.write().insert("in1".into(), 7.0);

        apply_tab_graph(
            &state,
            None,
            "tab1".into(),
            vec![input_node("in1", "tab1")],
            vec![],
            HashMap::new(),
            None,
            None,
            None,
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
        state.input_values.write().insert("in1".into(), 3.0);
        apply_tab_graph(
            &state,
            None,
            "tab1".into(),
            vec![input_node("in1", "tab1")],
            vec![],
            HashMap::new(),
            None,
            None,
            None,
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
        assert!(
            state.source_graphs.lock().get("tab1").is_none(),
            "tab 移除后源图存储应同步清除"
        );
    }

    // ---- 源图存储 / 版本冲突 / 拓扑 op ----

    fn protocol_node(id: &str, tab_id: &str) -> NodeDef {
        NodeDef {
            id: id.into(),
            tab_id: tab_id.into(),
            kind: NodeKind::Protocol {
                config: schema_types::ProtocolConfig::JustFloat { channels: None },
                convert_to: None,
                schema: None,
            },
        }
    }

    fn math_node(id: &str, tab_id: &str) -> NodeDef {
        NodeDef {
            id: id.into(),
            tab_id: tab_id.into(),
            kind: NodeKind::Math {
                op: node_kind::MathOp::Add,
                input_count: 1,
            },
        }
    }

    fn sink_node(id: &str, tab_id: &str) -> NodeDef {
        NodeDef {
            id: id.into(),
            tab_id: tab_id.into(),
            kind: NodeKind::Sink,
        }
    }

    fn edge(id: &str, source: &str, sh: &str, target: &str, th: &str) -> Edge {
        Edge {
            id: id.into(),
            source: source.into(),
            source_handle: sh.into(),
            target: target.into(),
            target_handle: th.into(),
        }
    }

    /// 提交成功写入源图存储 + 版本号递增; base_version 过期返回版本冲突
    #[tokio::test]
    async fn update_tab_graph_writes_source_store_and_checks_version() {
        let state = AppState::new();
        let derived = apply_tab_graph(
            &state,
            None,
            "tab1".into(),
            vec![input_node("in1", "tab1")],
            vec![],
            HashMap::new(),
            None,
            None,
            None,
        )
        .await
        .expect("提交图应成功");
        assert_eq!(derived.version, 1, "首次提交版本号应为 1");
        assert_eq!(
            state
                .source_graphs
                .lock()
                .get("tab1")
                .map(|g| g.nodes.len()),
            Some(1),
            "成功提交应写入源图存储"
        );

        // 过期 base_version → GraphVersionConflict (其他写入方推进了版本)
        let err = apply_tab_graph(
            &state,
            None,
            "tab1".into(),
            vec![input_node("in1", "tab1")],
            vec![],
            HashMap::new(),
            None,
            None,
            Some(0),
        )
        .await
        .expect_err("过期版本应冲突");
        assert!(
            err.to_string().contains("版本冲突"),
            "应报告版本冲突: {err}"
        );

        // 匹配的 base_version → 成功且版本推进
        let derived = apply_tab_graph(
            &state,
            None,
            "tab1".into(),
            vec![input_node("in1", "tab1")],
            vec![],
            HashMap::new(),
            None,
            None,
            Some(1),
        )
        .await
        .expect("匹配版本应成功");
        assert_eq!(derived.version, 2);
    }

    /// 编译失败必须返回真实 CompileError (域不匹配可读原因), 不再是占位 Cycle 假错误;
    /// 且源图存储不变 (提交被整体拒绝)
    #[tokio::test]
    async fn update_tab_graph_returns_real_compile_error() {
        let state = AppState::new();
        let err = apply_tab_graph(
            &state,
            None,
            "tab1".into(),
            vec![protocol_node("pt", "tab1"), math_node("m1", "tab1")],
            vec![edge("e1", "pt", "out", "m1", "in0")],
            HashMap::new(),
            None,
            None,
            None,
        )
        .await
        .expect_err("Protocol.out(bytes) → Math.in0(f32) 应域不匹配");
        let msg = err.to_string();
        assert!(msg.contains("域不匹配"), "真实原因应可见: {msg}");
        assert!(!msg.contains("循环"), "不得回落为占位循环错误: {msg}");
        assert!(
            state.source_graphs.lock().get("tab1").is_none(),
            "失败提交不得写入源图存储"
        );
    }

    /// connect_edge op: 默认 handle 按端口提示解析、RawData 目标改写 src: 端口、
    /// 等价边幂等; 域不匹配被编译拒绝且源图不变
    #[tokio::test]
    async fn connect_edge_op_validates_and_persists() {
        use app_state::SourceNodeHint;
        let state = AppState::new();
        let mut hints = HashMap::new();
        hints.insert(
            "in1".to_string(),
            SourceNodeHint {
                default_input: None,
                default_output: Some("value".into()),
                raw_data: false,
            },
        );
        hints.insert(
            "m1".to_string(),
            SourceNodeHint {
                default_input: Some("in0".into()),
                default_output: Some("result".into()),
                raw_data: false,
            },
        );
        apply_tab_graph(
            &state,
            None,
            "tab1".into(),
            vec![input_node("in1", "tab1"), math_node("m1", "tab1")],
            vec![],
            hints,
            None,
            None,
            None,
        )
        .await
        .expect("种子图应成功");

        // 默认 handle: in1.value → m1.in0
        let out = crate::apply_connect_edge(
            &state.graphs,
            &state.graphs_version,
            &state.data_plane,
            &state.source_graphs,
            &state.workspace,
            None,
            None,
            "in1".into(),
            "m1".into(),
            None,
            None,
        )
        .await
        .expect("默认 handle 连线应成功");
        let stored = state.source_graphs.lock().get("tab1").unwrap().clone();
        assert_eq!(stored.edges.len(), 1);
        assert_eq!(stored.edges[0].source_handle, "value");
        assert_eq!(stored.edges[0].target_handle, "in0");

        // 等价边幂等 — 返回同一边 id, 不重复建边
        let again = crate::apply_connect_edge(
            &state.graphs,
            &state.graphs_version,
            &state.data_plane,
            &state.source_graphs,
            &state.workspace,
            None,
            None,
            "in1".into(),
            "m1".into(),
            None,
            None,
        )
        .await
        .expect("等价连线应幂等成功");
        assert_eq!(again.edge_id, out.edge_id);
        assert_eq!(
            state.source_graphs.lock().get("tab1").unwrap().edges.len(),
            1
        );

        // 域不匹配: m1.result (f32) → in1 (Input 无输入口, 端口域回退 f32 → 可编译)。
        // 改用明确的域冲突: 新建 protocol + math 再连 out → in0
        let mut hints2 = HashMap::new();
        hints2.insert(
            "pt".to_string(),
            SourceNodeHint {
                default_input: Some("in".into()),
                default_output: Some("out".into()),
                raw_data: false,
            },
        );
        apply_tab_graph(
            &state,
            None,
            "tab2".into(),
            vec![protocol_node("pt", "tab2"), math_node("m2", "tab2")],
            vec![],
            hints2,
            None,
            None,
            None,
        )
        .await
        .expect("tab2 种子图应成功");
        let err = crate::apply_connect_edge(
            &state.graphs,
            &state.graphs_version,
            &state.data_plane,
            &state.source_graphs,
            &state.workspace,
            None,
            None,
            "pt".into(),
            "m2".into(),
            None,
            Some("in0".into()),
        )
        .await
        .expect_err("Protocol.out(bytes) → Math.in0(f32) 应被编译拒绝");
        assert!(
            err.to_string().contains("域不匹配"),
            "应回传真实原因: {err}"
        );
        assert_eq!(
            state.source_graphs.lock().get("tab2").unwrap().edges.len(),
            0,
            "编译失败源图不得改变"
        );

        // RawData 目标: 端口提示 raw_data=true → target_handle 改写为 src:<source>:<handle>
        let mut hints3 = HashMap::new();
        hints3.insert(
            "in1".to_string(),
            SourceNodeHint {
                default_input: None,
                default_output: Some("value".into()),
                raw_data: false,
            },
        );
        hints3.insert(
            "raw1".to_string(),
            SourceNodeHint {
                default_input: Some("data".into()),
                default_output: None,
                raw_data: true,
            },
        );
        apply_tab_graph(
            &state,
            None,
            "tab3".into(),
            vec![input_node("in1", "tab3"), sink_node("raw1", "tab3")],
            vec![],
            hints3,
            None,
            None,
            None,
        )
        .await
        .expect("tab3 种子图应成功");
        crate::apply_connect_edge(
            &state.graphs,
            &state.graphs_version,
            &state.data_plane,
            &state.source_graphs,
            &state.workspace,
            None,
            None,
            "in1".into(),
            "raw1".into(),
            None,
            None,
        )
        .await
        .expect("RawData 连线应成功");
        let stored3 = state.source_graphs.lock().get("tab3").unwrap().clone();
        assert_eq!(stored3.edges[0].target_handle, "src:in1:value");
    }

    /// disconnect_edge op: 按 edge_id 删除并重编译; 未命中返回 GraphEdgeNotFound
    #[tokio::test]
    async fn disconnect_edge_op_removes_and_reports_miss() {
        let state = AppState::new();
        apply_tab_graph(
            &state,
            None,
            "tab1".into(),
            vec![input_node("in1", "tab1"), math_node("m1", "tab1")],
            vec![edge("e1", "in1", "value", "m1", "in0")],
            HashMap::new(),
            None,
            None,
            None,
        )
        .await
        .expect("种子图应成功");

        let out = crate::apply_disconnect_edge(
            &state.graphs,
            &state.graphs_version,
            &state.data_plane,
            &state.source_graphs,
            &state.workspace,
            None,
            Some("e1".into()),
            None,
            None,
        )
        .await
        .expect("按 edge_id 删边应成功");
        assert_eq!(out.edge_id, "e1");
        assert_eq!(
            state.source_graphs.lock().get("tab1").unwrap().edges.len(),
            0,
            "删除后源图不应再有该边"
        );

        let err = crate::apply_disconnect_edge(
            &state.graphs,
            &state.graphs_version,
            &state.data_plane,
            &state.source_graphs,
            &state.workspace,
            None,
            Some("ghost".into()),
            None,
            None,
        )
        .await
        .expect_err("未命中应报错");
        assert!(err.to_string().contains("未找到匹配的连线"));
    }
}
