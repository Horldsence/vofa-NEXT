//! `notify_events` — 前端事件契约 + 系统通知封装
//!
//! Stage H 拆分: 由 `src-tauri/src/events.rs` (前端事件契约) +
//! `src-tauri/src/notify.rs` (tauri-plugin-notification 封装) 合并而成。
//! 后续 Stage 还会并入菜单 / 更新器相关 emit 助手。
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
