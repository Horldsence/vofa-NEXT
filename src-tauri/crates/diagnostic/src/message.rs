//! 诊断事件统一枚举 — 跨 ISO-TP/UDS/OBD-II/J1939

use can_types::CanDirection;
use serde::{Deserialize, Serialize};

use crate::j1939::{J1939Id, J1939Spn};
use crate::obd::{Dtc, ObdMode};
use crate::uds::{UdsNrc, UdsService};

/// 诊断消息 — 跨 ISO-TP/UDS/OBD-II/J1939 的统一事件模型
///
/// 序列化采用 internally-tagged,前端可按 `kind` 字段判别联合类型:
/// `{ "kind": "UdsRequest", "service": "DiagnosticSessionControl", "sub_func": 3, "data": [...] }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DiagnosticMessage {
    /// ISO-TP 原始事件 (调试用)
    IsoTpFrame {
        timestamp: u64,
        tx_id: u32,
        rx_id: u32,
        data: Vec<u8>,
        direction: CanDirection,
    },

    /// UDS 请求
    UdsRequest {
        timestamp: u64,
        service: UdsService,
        sub_func: u8,
        data: Vec<u8>,
    },

    /// UDS 肯定响应
    UdsResponse {
        timestamp: u64,
        service: UdsService,
        data: Vec<u8>,
    },

    /// UDS 否定响应 (NRC)
    UdsErrorResponse {
        timestamp: u64,
        service: UdsService,
        nrc: UdsNrc,
    },

    /// OBD-II 请求
    ObdRequest {
        timestamp: u64,
        mode: ObdMode,
        pid: u8,
    },

    /// OBD-II PID 解码值
    ObdPidValue {
        timestamp: u64,
        mode: ObdMode,
        pid: u8,
        value: f32,
        unit: String,
    },

    /// OBD-II DTC 列表 (Mode 03/07/0A 响应)
    ObdDtcList { timestamp: u64, dtcs: Vec<Dtc> },

    /// J1939 PGN 完整报文
    J1939Pgn {
        timestamp: u64,
        id: J1939Id,
        data: Vec<u8>,
    },

    /// J1939 SPN 解码值 (一条 PGN 可产出多个 SPN)
    J1939Spn {
        timestamp: u64,
        pgn: u32,
        spns: Vec<J1939Spn>,
    },
}

impl DiagnosticMessage {
    /// 事件时间戳
    pub const fn timestamp(&self) -> u64 {
        match self {
            Self::IsoTpFrame { timestamp, .. }
            | Self::UdsRequest { timestamp, .. }
            | Self::UdsResponse { timestamp, .. }
            | Self::UdsErrorResponse { timestamp, .. }
            | Self::ObdRequest { timestamp, .. }
            | Self::ObdPidValue { timestamp, .. }
            | Self::ObdDtcList { timestamp, .. }
            | Self::J1939Pgn { timestamp, .. }
            | Self::J1939Spn { timestamp, .. } => *timestamp,
        }
    }
}

/// 诊断消息批次 — 一次推送多条消息 (与 `CanFrameBatch` 同构)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiagnosticMessageBatch {
    pub messages: Vec<DiagnosticMessage>,
}

impl DiagnosticMessageBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, msg: DiagnosticMessage) {
        self.messages.push(msg);
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}
