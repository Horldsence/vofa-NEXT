use crate::core::state::AppState;
use vofa_next_buffer::graph::Edge;
use vofa_next_core::Result;
use vofa_next_nodes::NodeDef;

// ============ 节点图 (后端化重构) ============

/// 更新指定 tab 的节点图 (整体替换 nodes + edges)
///
/// 编译失败 (循环等) 返回错误, 旧图保留
pub async fn update_tab_graph(
    state: &AppState,
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
pub async fn remove_tab_graph(state: &AppState, tab_id: String) -> Result<()> {
    state.graphs.lock().remove(&tab_id);
    Ok(())
}

/// 设置输入控件当前值 (Knob/Slider/Button/Radio/Checkbox 拖动时调用)
///
/// 该值会在下一帧 evaluate 时作为 Input 节点的输出
pub async fn set_input_value(state: &AppState, widget_id: String, value: f32) -> Result<()> {
    state.input_values.lock().insert(widget_id, value);
    Ok(())
}

/// 提交 Custom widget 的输出
///
/// 后端在下一帧 evaluate 时使用这些值作为 Custom 节点的输出
pub async fn submit_custom_output(
    state: &AppState,
    widget_id: String,
    outputs: std::collections::HashMap<String, f32>,
) -> Result<()> {
    state.custom_outputs.lock().insert(widget_id, outputs);
    Ok(())
}
