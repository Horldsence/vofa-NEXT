use crate::state::{AppState, CustomInputBatch, GraphOutputSnapshot, SpectrumBatch};
use tauri::{ipc::Channel, State};
use vofa_next_buffer::graph::Edge;
use vofa_next_core::Result;
use vofa_next_nodes::NodeDef;

// ============ 节点图 (后端化重构) ============

/// 更新指定 tab 的节点图 (整体替换 nodes + edges)
///
/// 编译失败 (循环等) 返回错误, 旧图保留
#[tauri::command]
pub async fn update_tab_graph(
    state: State<'_, AppState>,
    tab_id: String,
    nodes: Vec<NodeDef>,
    edges: Vec<Edge>,
) -> Result<()> {
    let compiled = vofa_next_nodes::CompiledGraph::compile(tab_id.clone(), nodes, edges)
        .map_err(|e| vofa_next_core::Error::Config(format!("{}", e)))?;
    let mut graphs = state.graphs.lock();
    graphs.insert(tab_id, compiled);
    Ok(())
}

/// 移除指定 tab 的节点图 (tab 删除时调用)
#[tauri::command]
pub async fn remove_tab_graph(state: State<'_, AppState>, tab_id: String) -> Result<()> {
    state.graphs.lock().remove(&tab_id);
    Ok(())
}

/// 设置输入控件当前值 (Knob/Slider/Button/Radio/Checkbox 拖动时调用)
///
/// 该值会在下一帧 evaluate 时作为 Input 节点的输出
#[tauri::command]
pub async fn set_input_value(
    state: State<'_, AppState>,
    widget_id: String,
    value: f32,
) -> Result<()> {
    state.input_values.lock().insert(widget_id, value);
    Ok(())
}

/// 提交 Custom widget 的输出 (前端 iframe 调用 ctx.send 后回传)
///
/// 后端在下一帧 evaluate 时使用这些值作为 Custom 节点的输出
#[tauri::command]
pub async fn submit_custom_output(
    state: State<'_, AppState>,
    widget_id: String,
    outputs: std::collections::HashMap<String, f32>,
) -> Result<()> {
    state.custom_outputs.lock().insert(widget_id, outputs);
    Ok(())
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
