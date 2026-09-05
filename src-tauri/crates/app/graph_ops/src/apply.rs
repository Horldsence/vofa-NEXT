//! 图提交核心 — tab 图整体编译提交 / 全局 BytePlan 重建
//!
//! 层级: L3 app。不含任何 `#[tauri::command]`; `app` 参数仅用于可选的事件
//! emit (`None` 供测试与启动恢复静默提交)。

use app_state::{
    prune_positions, AppState, Position, SourceGraphs, SourceNodeHint, TabSourceGraph,
    WidgetRecord, WorkspaceState,
};
use buffer_graph::Edge;
use data_plane::data_plane::frame_dispatch;
use data_plane::decoder_feed::sync_decoders_now;
use data_plane::DataPlaneState;
use engine::BytePlan;
use error::ConfigError;
use kind::NodeDef;
use notify_events::emit_graph_derived;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use vofa_core::{Error, Result};

use crate::{
    compute_derived, inject_protocol_sources, CompileState, GraphCompileEvent, GraphDerived,
    GraphSourceEvent, GRAPH_SOURCE_EVENT,
};

// ============ 节点图 (后端化重构) ============

/// 用候选全局节点表 + 全部 tab 的字节边重建全局 BytePlan
///
/// 简单合并策略: 全局节点表按 id 覆盖合并 (任何 tab 提交后重建全局平面);
/// 孤儿节点 (图删除后残留) 的运行时资源由 `DataPlaneState::reconcile` 清理。
fn rebuild_byte_plan(
    graphs: &Arc<parking_lot::Mutex<HashMap<String, engine::CompiledGraph>>>,
    candidate: &std::collections::HashMap<String, NodeDef>,
    new_tab: Option<(&str, &engine::CompiledGraph)>,
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
    let typed = engine::TypedGraph::build(candidate.values().cloned(), byte_edges)
        .map_err(|e| Error::Config(ConfigError::BytePlanCompile(Box::new(e))))?;
    BytePlan::build(&typed).map_err(|e| Error::Config(ConfigError::BytePlanCompile(Box::new(e))))
}

/// 更新指定 tab 的节点图 (整体替换 nodes + edges) — Tauri 命令入口在
/// `graph::update_tab_graph`, 本体抽出以便不依赖 Tauri State 地复用与测试
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
    let derived = apply_tab_graph_parts(
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
    .await?;
    state.prune_waveform_snapshots();
    Ok(derived)
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
    graphs: &Arc<parking_lot::Mutex<HashMap<String, engine::CompiledGraph>>>,
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
        match engine::CompiledGraph::compile(tab_id.clone(), compile_nodes, edges.clone()) {
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
        if matches!(n.kind, kind::NodeKind::ProtocolSource { .. }) {
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
    dispatcher::sync_spectrum_analyzers(&data_plane.eval);
    dispatcher::sync_ifft_buffers(&data_plane.eval);

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

/// 移除指定 tab 的节点图 (tab 删除时调用) — Tauri 命令入口在
/// `graph::remove_tab_graph`, 本体抽出以便复用与测试
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
    state.prune_waveform_snapshots();
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
