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
    state
        .graphs_version
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// 移除指定 tab 的节点图 (tab 删除时调用)
#[tauri::command]
pub async fn remove_tab_graph(state: State<'_, AppState>, tab_id: String) -> Result<()> {
    state.graphs.lock().remove(&tab_id);
    state
        .graphs_version
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// 设置输入控件当前值 (Knob/Slider/Button/Radio/Checkbox 拖动时调用)
///
/// 该值会在下一帧 evaluate 时作为 Input 节点的输出
///
/// 立即空帧 evaluate 一次: 输入控件的值变化必须即时反映到图输出,
/// 不能依赖 transport 数据流 — 断开/无帧时下游 (CommandSender onChange 发送、
/// Gauge 等显示控件) 也要能感知变化。
#[tauri::command]
pub async fn set_input_value(
    state: State<'_, AppState>,
    widget_id: String,
    value: f32,
) -> Result<()> {
    state.input_values.lock().insert(widget_id, value);
    crate::pipeline::graph_eval::evaluate_all_graphs_with(
        &state.eval_state(),
        &vofa_next_core::DataFrame::new(vec![]),
    );
    Ok(())
}

/// 提交 Custom widget 的输出 (前端 iframe 调用 ctx.send 后回传)
///
/// 后端在下一帧 evaluate 时使用这些值作为 Custom 节点的输出
/// (同 set_input_value: 立即 evaluate, 不依赖 transport 数据流)
#[tauri::command]
pub async fn submit_custom_output(
    state: State<'_, AppState>,
    widget_id: String,
    outputs: std::collections::HashMap<String, f32>,
) -> Result<()> {
    state.custom_outputs.lock().insert(widget_id, outputs);
    crate::pipeline::graph_eval::evaluate_all_graphs_with(
        &state.eval_state(),
        &vofa_next_core::DataFrame::new(vec![]),
    );
    Ok(())
}

/// 回环字节注入 — CommandSender 回环模式的发送路径
///
/// 将字节沿回环边 (target_handle == loopbackIn) 路由到连线的 FrameDecoder:
/// 1. 在所有 tab 图中查找 loopback_targets_for(source_widget_id)
/// 2. 对每个目标解码器 feed_one_decoder (ensure parser + feed + 旁路收集)
/// 3. 空帧 evaluate 一次, 刷新 output_snapshot (60 FPS ticker 推送前端)
///
/// 与串口开关无关 — 不触碰 transport, 无连接时 data_loop 不跑也能工作。
///
/// 返回: 实际注入的解码器数量 (0 = 未连线, 前端可忽略)
#[tauri::command]
pub async fn inject_loopback_bytes(
    state: State<'_, AppState>,
    source_widget_id: String,
    data: Vec<u8>,
) -> Result<usize> {
    let ts_us = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);

    // 1. 收集 (decoder_id, parse_config) — 先取快照再逐个 feed, 避免长时持锁
    let targets: Vec<(String, crate::pipeline::decoder_feed::DecoderParseConfig)> = {
        let graphs = state.graphs.lock();
        let mut v = Vec::new();
        for (_, graph) in graphs.iter() {
            for dec_id in graph.loopback_targets_for(&source_widget_id) {
                if let Some(cfg) = graph.decoder_config(&dec_id) {
                    v.push((dec_id, (cfg.0.to_vec(), cfg.1, cfg.2, cfg.3, cfg.4)));
                }
            }
        }
        v
    };

    // 2. 逐个喂入 (与 live 帧同等对待: 更新 last_frame + 旁路收集)
    let eval_state = state.eval_state();
    for (dec_id, config) in &targets {
        crate::pipeline::decoder_feed::feed_one_decoder(&eval_state, dec_id, config, &data, ts_us);
    }

    // 3. 空帧 evaluate — decoder 输出来自 last_frame 缓存 (先例: data_loop.rs 空闲帧求值)
    if !targets.is_empty() {
        crate::pipeline::graph_eval::evaluate_all_graphs_with(
            &eval_state,
            &vofa_next_core::DataFrame::new(vec![]),
        );
    }

    Ok(targets.len())
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
