//! 共享工具实现 — [`VofaMcpServer`](crate::server::VofaMcpServer) 的 rmcp handler
//! 与内置 AI 原生工具执行器 (`ai::native_executor`) 共用的普通异步函数层。
//!
//! 统一返回 `Result<Value, String>`:rmcp 侧映射为 MCP 错误,原生执行器侧
//! 映射为工具失败回填 (is_error),两侧零重复。

use std::collections::HashMap;
use std::sync::atomic::Ordering;

use buffer_raw::RawDataDirection;
use can_types::CanFrame;
use data_plane::data_plane::{byte_router, frame_dispatch};
use data_plane::decoder_feed::DecoderFeedCache;
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::server::Toolbox;

/// 波形读取点数上限 (防止超长返回撑爆上下文)。
const MAX_WAVEFORM_POINTS: u32 = 10_000;
/// CAN 帧读取条数上限。
const MAX_CAN_FRAMES: u32 = 1_000;
/// 逻辑采样 / 解码事件读取条数上限。
const MAX_LOGIC_ITEMS: u32 = 5_000;
/// 原始字节读取量上限。
const MAX_RAW_BYTES: u32 = 64 * 1024;
/// 原始数据默认读取的最近块数。
const RAW_DEFAULT_CHUNKS: usize = 32;

/// 列出全部传输节点及其连接状态。
pub async fn list_transports(tb: &Toolbox) -> Value {
    let mgr = tb.transport.lock().await;
    let list: Vec<Value> = mgr
        .list_open()
        .into_iter()
        .map(|node_id| {
            json!({
                "node_id": node_id,
                "state": mgr.state(&node_id).map(|s| format!("{s:?}")).unwrap_or_default(),
            })
        })
        .collect();
    json!({ "transports": list })
}

/// 向传输节点发送原始字节,返回发送字节数。
pub async fn send_bytes(tb: &Toolbox, node_id: &str, data: &[u8]) -> Result<Value, String> {
    let len = data.len();
    tb.transport
        .lock()
        .await
        .send(node_id, data)
        .map_err(|e| e.to_string())?;
    push_tx_raw(tb, node_id, data);
    Ok(json!({ "sent_bytes": len }))
}

/// 向传输节点发送 UTF-8 文本,返回发送字节数。
pub async fn send_string(tb: &Toolbox, node_id: &str, text: &str) -> Result<Value, String> {
    send_bytes(tb, node_id, text.as_bytes()).await
}

/// 字节注入 — 沿全局字节平面路由 (喂协议引擎 / FrameDecoder / Transport.tx)。
pub async fn inject_bytes(
    tb: &Toolbox,
    app: &AppHandle,
    source_node_id: &str,
    data: &[u8],
) -> Result<Value, String> {
    let plane = tb.data_plane.clone();
    let hit = plane.byte_plan.lock().routes_for(source_node_id).len();
    let mut cache = DecoderFeedCache::new();
    let summary =
        byte_router::route_bytes(&plane, Some(app), source_node_id, data, 0, &mut cache, None)
            .await;
    if summary.decoders_fed {
        frame_dispatch::refresh_snapshot(&plane);
    }
    Ok(json!({ "routed_targets": hit }))
}

/// 设置控件输入值 (Input/Slider/Knob 等 widget 的当前值)。
pub fn set_input_value(tb: &Toolbox, widget_id: &str, value: f32) -> Value {
    tb.input_values.write().insert(widget_id.to_string(), value);
    frame_dispatch::refresh_snapshot(&tb.data_plane);
    json!({ "ok": true })
}

/// 读取图输出快照 (全部节点输出端口的最新值)。
pub fn get_graph_outputs(tb: &Toolbox) -> Value {
    let snapshot = tb.data_plane.eval.output_snapshot.lock();
    let values = snapshot
        .values
        .iter()
        .map(|(widget, ports)| {
            (
                widget.clone(),
                ports
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect::<Value>(),
            )
        })
        .collect::<Value>();
    json!({ "tick": snapshot.tick, "outputs": values })
}

/// 读取指定数据源最近 count 个采样点的波形窗口。
pub fn get_recent_waveform(tb: &Toolbox, source: &str, count: u32) -> Result<Value, String> {
    let buf = tb.data_plane.buffer_for(source);
    let window = buf
        .lock()
        .get_recent(count.clamp(1, MAX_WAVEFORM_POINTS) as usize);
    serde_json::to_value(&window).map_err(|e| e.to_string())
}

/// 读取指定数据源时间窗口内的波形 (start/end 为相对最新时间戳的毫秒偏移)。
pub fn get_waveform_window(
    tb: &Toolbox,
    source: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Value, String> {
    let buf = tb.data_plane.buffer_for(source);
    let window = buf.lock().get_window(start_ms, end_ms);
    serde_json::to_value(&window).map_err(|e| e.to_string())
}

/// 读取缓冲区信息 (通道数与点数)。
pub fn get_buffer_info(tb: &Toolbox, source: &str) -> Value {
    let buf = tb.data_plane.buffer_for(source);
    let b = buf.lock();
    json!({ "channel_count": b.channel_count(), "point_count": b.point_count() })
}

/// 列出存在波形缓冲的数据源 id。
pub fn list_data_sources(tb: &Toolbox) -> Value {
    let keys: Vec<String> = tb.data_plane.buffers.lock().keys().cloned().collect();
    json!({ "sources": keys })
}

/// 列出已提交节点图的 tab id。
pub fn list_tabs(tb: &Toolbox) -> Value {
    let tabs: Vec<String> = tb.graphs.lock().keys().cloned().collect();
    json!({ "tabs": tabs })
}

/// 提交 (替换) 指定 tab 的节点图 — 与前端提交同一路径,返回派生端口表。
///
/// `widgets` / `positions` 为可选的控件配置记录与画布位置 (widget 配置模型
/// 的后端权威存储):提供时画布可完整渲染控件,缺省保留现状。
pub async fn update_graph(
    tb: &Toolbox,
    app: &AppHandle,
    tab_id: &str,
    nodes: Vec<Value>,
    edges: Vec<Value>,
    widgets: Option<Vec<Value>>,
    positions: Option<HashMap<String, Value>>,
) -> Result<Value, String> {
    #![allow(clippy::implicit_hasher)] // serde_json::Value 反序列化约定 std hasher, 仅内部工具入口

    let nodes: Vec<kind::NodeDef> = nodes
        .into_iter()
        .map(serde_json::from_value)
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| format!("nodes 反序列化失败: {e}"))?;
    let edges: Vec<buffer_graph::Edge> = edges
        .into_iter()
        .map(serde_json::from_value)
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| format!("edges 反序列化失败: {e}"))?;
    let widgets: Option<Vec<app_state::WidgetRecord>> = match widgets {
        Some(items) => Some(
            items
                .into_iter()
                .map(serde_json::from_value)
                .collect::<std::result::Result<_, _>>()
                .map_err(|e| format!("widgets 反序列化失败: {e}"))?,
        ),
        None => None,
    };
    let positions: Option<HashMap<String, app_state::Position>> = match positions {
        Some(map) => Some(
            map.into_iter()
                .map(|(id, v)| serde_json::from_value::<app_state::Position>(v).map(|p| (id, p)))
                .collect::<std::result::Result<_, _>>()
                .map_err(|e| format!("positions 反序列化失败: {e}"))?,
        ),
        None => None,
    };

    let derived = graph_ops::apply_tab_graph_parts(
        &tb.graphs,
        &tb.graphs_version,
        &tb.data_plane,
        &tb.source_graphs,
        &tb.workspace,
        Some(app),
        tab_id.to_string(),
        nodes,
        edges,
        HashMap::default(),
        widgets,
        positions,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    serde_json::to_value(&derived).map_err(|e| e.to_string())
}

/// 连线 — 连线拓扑的后端权威入口 (与内置 AI connect_nodes 同一实现)。
///
/// handle 省略时按端口提示或节点类型补默认;RawData 控件目标自动改写
/// `src:<source>:<handle>`。编译失败 (环/端口域不匹配) 返回真实原因, 不建边。
pub async fn connect_edge(
    tb: &Toolbox,
    app: &AppHandle,
    tab_id: Option<String>,
    source: &str,
    target: &str,
    source_handle: Option<String>,
    target_handle: Option<String>,
) -> Result<Value, String> {
    let out = graph_ops::apply_connect_edge(
        &tb.graphs,
        &tb.graphs_version,
        &tb.data_plane,
        &tb.source_graphs,
        &tb.workspace,
        Some(app),
        tab_id,
        source.to_string(),
        target.to_string(),
        source_handle,
        target_handle,
    )
    .await
    .map_err(|e| e.to_string())?;
    serde_json::to_value(&out).map_err(|e| e.to_string())
}

/// 删线 — 按 edge_id 或 source/target 组合 (可只给一端) 查找删除。
pub async fn disconnect_edge(
    tb: &Toolbox,
    app: &AppHandle,
    edge_id: Option<String>,
    source: Option<String>,
    target: Option<String>,
) -> Result<Value, String> {
    let out = graph_ops::apply_disconnect_edge(
        &tb.graphs,
        &tb.graphs_version,
        &tb.data_plane,
        &tb.source_graphs,
        &tb.workspace,
        Some(app),
        edge_id,
        source,
        target,
    )
    .await
    .map_err(|e| e.to_string())?;
    serde_json::to_value(&out).map_err(|e| e.to_string())
}

/// 读取最近 CAN 帧 + 负载统计 (bitrate 缺省 500k, 仅用于负载百分比估算)。
pub fn get_can_frames(tb: &Toolbox, count: u32, bitrate: Option<u32>) -> Value {
    let frames = tb
        .can_buffer
        .lock()
        .get_recent(count.clamp(1, MAX_CAN_FRAMES) as usize);
    let snapshot = tb
        .can_load_stats
        .lock()
        .snapshot(bitrate.unwrap_or(500_000));
    json!({
        "frames": frames,
        "count": frames.len(),
        "load": snapshot,
    })
}

/// 发送 CAN 帧 — 经指定 (或自动溯源的第一个) Protocol 节点 encode_can 编码后发送。
pub async fn send_can_frame(
    tb: &Toolbox,
    node_id: &str,
    protocol_node: Option<String>,
    frame: CanFrame,
) -> Result<Value, String> {
    let plane = tb.data_plane.clone();
    let proto_id = match protocol_node {
        Some(p) => p,
        None => {
            let routes = plane.byte_plan.lock().routes_for(node_id).to_vec();
            let nodes = plane.global_nodes.lock();
            routes
                .iter()
                .find_map(|r| {
                    matches!(
                        nodes.get(&r.target).map(|n| &n.kind),
                        Some(kind::NodeKind::Protocol { .. })
                    )
                    .then(|| r.target.clone())
                })
                .ok_or_else(|| "未找到该传输下游的 Protocol 节点, 无法编码 CAN 帧".to_string())?
        }
    };
    let data = {
        let st = plane
            .protocol_states
            .lock()
            .get(&proto_id)
            .cloned()
            .ok_or_else(|| format!("Protocol 节点不存在: {proto_id}"))?;
        let engine = st.lock().engine.clone();
        let bytes = engine.lock().encode_can(&frame);
        bytes
    };
    if data.is_empty() {
        return Err("该 Protocol 节点不是 CAN 协议 (encode_can 为空)".to_string());
    }
    tb.transport
        .lock()
        .await
        .send(node_id, &data)
        .map_err(|e| e.to_string())?;
    Ok(json!({ "sent_bytes": data.len(), "protocol_node": proto_id }))
}

/// 读取最近逻辑采样与解码事件 (UART/I2C/SPI 等)。
pub fn get_logic_data(tb: &Toolbox, count: u32) -> Value {
    let n = count.clamp(1, MAX_LOGIC_ITEMS) as usize;
    let samples = tb.logic_buffer.lock().get_recent(n);
    let events = tb.decoded_buffer.lock().get_recent(n);
    json!({
        "samples": samples,
        "sample_count": samples.len(),
        "decoded_events": events,
        "event_count": events.len(),
    })
}

/// 读取指定源的最近原始字节 (TX/RX 分方向, hex 编码)。
pub fn get_raw_data(tb: &Toolbox, source: &str, max_bytes: u32) -> Value {
    let collector = tb.data_plane.raw_collector_for(source);
    let c = collector.lock();
    let max_bytes = max_bytes.clamp(1, MAX_RAW_BYTES) as usize;
    // 读尾部: 从 (总块数 - 默认块数) 对应的绝对索引起读
    let start = c
        .base_index()
        .saturating_add(c.chunk_count().saturating_sub(RAW_DEFAULT_CHUNKS));
    let (chunks, next_index) = c.read_from(start, max_bytes);
    let items: Vec<Value> = chunks
        .iter()
        .map(|(ts, dir, bytes)| {
            json!({
                "timestamp_us": ts,
                "direction": match dir {
                    RawDataDirection::Rx => "rx",
                    RawDataDirection::Tx => "tx",
                },
                "hex": hex_encode(bytes),
                "len": bytes.len(),
            })
        })
        .collect();
    json!({
        "chunks": items,
        "next_index": next_index,
        "dropped_bytes": c.dropped_bytes(),
        "total_bytes": c.total_bytes(),
    })
}

/// 列出可用串口 (名称/类型/USB 信息)。
pub fn list_serial_ports() -> Result<Value, String> {
    let ports = serial::serial::list_ports().map_err(|e| e.to_string())?;
    serde_json::to_value(&ports).map_err(|e| e.to_string())
}

/// TX 字节进该源 raw 收集器 (与 `send_raw` 命令保持统计口径一致)。
fn push_tx_raw(tb: &Toolbox, node_id: &str, data: &[u8]) {
    tb.data_plane.raw_collector_for(node_id).lock().push_chunk(
        vofa_core::now_us(),
        RawDataDirection::Tx,
        data,
    );
}

/// 字节 → hex 字符串 (空格分隔, 与前端 RawData 视图风格一致)。
fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        // String 的 write! 实现不会失败, 直接忽略返回值
        let _ = write!(s, "{b:02X}");
    }
    s
}

/// 工具调用序号 (前端托管调用 id 生成)。
static CALL_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 生成前端托管工具调用的唯一 id。
pub fn next_call_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "fe-{:x}-{:x}",
        nanos,
        CALL_SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// hex 编码: 空格分隔大写, 空输入为空串。
    #[test]
    fn hex_encode_formats_bytes() {
        assert_eq!(hex_encode(&[]), "");
        assert_eq!(hex_encode(&[0x00, 0xAA, 0xFF]), "00 AA FF");
    }

    /// 调用 id 唯一且带 fe- 前缀。
    #[test]
    fn call_ids_are_unique() {
        let a = next_call_id();
        let b = next_call_id();
        assert_ne!(a, b);
        assert!(a.starts_with("fe-") && b.starts_with("fe-"));
    }
}
