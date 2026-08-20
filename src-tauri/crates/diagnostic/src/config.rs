//! 诊断协议配置 — ISO-TP / UDS / OBD-II / J1939

use serde::{Deserialize, Serialize};

/// ISO-TP 地址模式 (Normal / Extended / Mixed)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum IsoTpAddressMode {
    #[default]
    Normal,
    Extended,
    Mixed,
}

/// ISO-TP 会话配置 (与 libautomotive `IsoTpConfig` 概念对齐)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IsoTpConfig {
    pub tx_id: u32,
    pub rx_id: u32,
    pub block_size: u8,
    pub st_min: u8,
    pub address_mode: IsoTpAddressMode,
    pub padding: Option<u8>,
    pub timeout_ms: u32,
}

impl Default for IsoTpConfig {
    fn default() -> Self {
        Self {
            tx_id: 0x7E0,
            rx_id: 0x7E8,
            block_size: 0,
            st_min: 0,
            address_mode: IsoTpAddressMode::Normal,
            padding: None,
            timeout_ms: 1000,
        }
    }
}

/// UDS 客户端配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UdsConfig {
    pub p2_timeout_ms: u32,
    pub tester_present_interval_ms: u32,
}

impl Default for UdsConfig {
    fn default() -> Self {
        Self {
            p2_timeout_ms: 5000,
            tester_present_interval_ms: 2000,
        }
    }
}

/// OBD-II 客户端配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObdConfig {
    /// 轮询间隔 (ms)
    pub poll_interval_ms: u32,
    /// 默认请求 ID (11-bit)
    pub default_request_id: u32,
    /// 默认响应 ID (11-bit)
    pub default_response_id: u32,
}

impl Default for ObdConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 100,
            default_request_id: 0x7DF,
            default_response_id: 0x7E8,
        }
    }
}

/// J1939 解码器配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct J1939Config {
    /// 默认源地址
    pub source_address: u8,
    /// 心跳周期 (ms)
    pub heartbeat_interval_ms: u32,
}

/// 诊断配置 — 用于 `ProtocolConfig::Diagnostic` 变体
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind")]
pub enum DiagnosticConfig {
    /// 仅 ISO-TP 透传 (调试用)
    IsoTp { config: IsoTpConfig },
    /// UDS 客户端
    Uds { isotp: IsoTpConfig, uds: UdsConfig },
    /// OBD-II 客户端
    Obd { isotp: IsoTpConfig, obd: ObdConfig },
    /// J1939 监听器 (不需要 ISO-TP,直接吃 CanFrame)
    J1939 { j1939: J1939Config },
}

impl Default for DiagnosticConfig {
    fn default() -> Self {
        Self::Uds {
            isotp: IsoTpConfig::default(),
            uds: UdsConfig::default(),
        }
    }
}
