//! `notify_events` — 前端事件契约 + 系统通知封装
//!
//! 层级: L0 foundation — 前端事件契约 + 系统通知封装; 禁止依赖任何上层。
//!
//! 数据平面读任务 ([`emit_transport_state`] / [`emit_transport_rx`]) 通过本 crate
//! 向前端发送传输连接状态变化与统计节流事件; Tauri 命令 (open_transport 等)
//! 通过 [`notify`] 模块向用户推送系统通知。

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use vofa_core::{ConnectionState, TransportStats};

/// `transport:state` 事件名
pub const TRANSPORT_STATE_EVENT: &str = "transport:state";
/// `transport:rx` 事件名 (统计节流推送)
pub const TRANSPORT_RX_EVENT: &str = "transport:rx";
/// `protocol:channels-detected` 事件名 (自动通道检测值变化推送)
pub const PROTOCOL_CHANNELS_DETECTED_EVENT: &str = "protocol:channels-detected";
/// `graph:derived` 事件名 (图编译派生数据 — 节点输出端口表 / 生效通道数, 差分推送)
pub const GRAPH_DERIVED_EVENT: &str = "graph:derived";
/// `graph:compile` event name (编译队列状态广播 — pending / ok / error, 携 receipt_seq)
pub const GRAPH_COMPILE_EVENT: &str = "graph:compile";

/// `transport:state` payload — 连接状态变化 (携带来源 Transport 节点 id)
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TransportStateEvent {
    pub node_id: String,
    pub state: ConnectionState,
}

/// `transport:rx` payload — 接收统计 (携带来源 Transport 节点 id)
///
/// 注意: 不派生 PartialEq (TransportStats 未实现)。
#[derive(Debug, Clone, Serialize)]
pub struct TransportRxEvent {
    pub node_id: String,
    pub stats: TransportStats,
}

/// emit `transport:state` (失败安全: 忽略 emit 错误)
pub fn emit_transport_state(app: &AppHandle, node_id: &str, state: ConnectionState) {
    let _ = app.emit(
        TRANSPORT_STATE_EVENT,
        TransportStateEvent {
            node_id: node_id.to_string(),
            state,
        },
    );
}

/// `protocol:channels-detected` payload — 自动通道检测值变化 (携带来源 Protocol 节点 id)
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProtocolChannelsDetectedEvent {
    pub node_id: String,
    pub channels: usize,
}

/// emit `transport:rx` (失败安全: 忽略 emit 错误)
pub fn emit_transport_rx(app: &AppHandle, node_id: &str, stats: TransportStats) {
    let _ = app.emit(
        TRANSPORT_RX_EVENT,
        TransportRxEvent {
            node_id: node_id.to_string(),
            stats,
        },
    );
}

/// emit `protocol:channels-detected` (失败安全: 忽略 emit 错误)
pub fn emit_protocol_channels_detected(app: &AppHandle, node_id: &str, channels: usize) {
    let _ = app.emit(
        PROTOCOL_CHANNELS_DETECTED_EVENT,
        ProtocolChannelsDetectedEvent {
            node_id: node_id.to_string(),
            channels,
        },
    );
}

/// `graph:derived` payload — 图编译派生数据 (节点输出端口表 / 生效通道数)
///
/// payload 结构由调用方 (`graph::derived::GraphDerived`) 提供 — 本 crate
/// 不依赖 graph 以避免循环依赖; emit 函数接受任意可序列化的 payload。
/// 调用方约定 payload 形如 `{ "nodes": [{ "node_id": "...", "ports": [...], "effective_channels": N }] }`。
pub fn emit_graph_derived<P: Serialize + Clone>(app: &AppHandle, payload: &P) {
    let _ = app.emit(GRAPH_DERIVED_EVENT, payload.clone());
}

/// emit `graph:compile` 事件 payload — 编译队列状态广播.
pub fn emit_graph_compiled<P: Serialize + Clone>(app: &AppHandle, payload: &P) {
    let _ = app.emit(GRAPH_COMPILE_EVENT, payload.clone());
}

pub mod notify;

#[cfg(test)]
mod tests {
    use super::*;

    /// 事件契约: payload JSON 结构必须与前端约定严格一致
    #[test]
    fn transport_state_event_json_shape() {
        let v = serde_json::to_value(TransportStateEvent {
            node_id: "tp1".into(),
            state: ConnectionState::Connected,
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({"node_id": "tp1", "state": "Connected"})
        );
    }

    #[test]
    fn protocol_channels_detected_event_json_shape() {
        let v = serde_json::to_value(ProtocolChannelsDetectedEvent {
            node_id: "pt1".into(),
            channels: 3,
        })
        .unwrap();
        assert_eq!(v, serde_json::json!({"node_id": "pt1", "channels": 3}));
    }

    #[test]
    fn transport_rx_event_json_shape() {
        let v = serde_json::to_value(TransportRxEvent {
            node_id: "tp1".into(),
            stats: TransportStats {
                rx_bytes: 10,
                tx_bytes: 2,
                rx_frames: 3,
                tx_frames: 1,
                rx_dropped: 0,
            },
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "node_id": "tp1",
                "stats": {
                    "rx_bytes": 10,
                    "tx_bytes": 2,
                    "rx_frames": 3,
                    "tx_frames": 1,
                    "rx_dropped": 0,
                }
            })
        );
    }
}
