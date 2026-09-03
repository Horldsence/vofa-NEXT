//! J1939 报文标识与 SPN

use serde::{Deserialize, Serialize};

/// J1939 报文标识 (优先级 / PGN / 源/目标地址)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct J1939Id {
    pub priority: u8,
    pub pgn: u32,
    pub source: u8,
    pub destination: u8,
}

/// J1939 SPN (Suspect Parameter Number) 解码值
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct J1939Spn {
    /// SPN 编号
    pub spn: u32,
    /// 可读名称
    pub name: String,
    /// 解码后的值
    pub value: f64,
    /// 单位 (如 "rpm", "kPa", "°C")
    pub unit: String,
}
