//! 前端事件契约 — 后端 emit 的事件名与 payload 结构
//!
//! 节点化重构后所有传输事件都携带来源节点 id, 前端按 node_id 路由到对应节点:
//! - `transport:state`: `{"node_id": "...", "state": <ConnectionState>}`
//! - `transport:rx`:    `{"node_id": "...", "stats": <TransportStats>}`

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use vofa_next_core::{ConnectionState, TransportStats};

/// `transport:state` 事件名
pub const TRANSPORT_STATE_EVENT: &str = "transport:state";
/// `transport:rx` 事件名 (统计节流推送)
pub const TRANSPORT_RX_EVENT: &str = "transport:rx";

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
