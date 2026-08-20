//! 协议配置 — schema 的 legacy_config / TestDataLink.protocol 用。
//!
//! 自 `vofa-next-core/src/config.rs` 迁出, 仅保留 schema 所需的最小集 (ProtocolConfig)。
//! 传输 / 控件 / 流水线配置仍由 `vofa-next-core::config` 承担, 待 Stage H 转 façade 时
//! 再统一迁入 `vofa_core::config`。

use diagnostic::DiagnosticConfig;
use logic_types::LogicDecoderConfig;
use serde::{Deserialize, Serialize};

/// 协议配置
/// channels: None = 自动检测通道数, Some(n) = 手动指定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind")]
pub enum ProtocolConfig {
    JustFloat {
        channels: Option<usize>,
    },
    FireWater {
        channels: Option<usize>,
    },
    RawData,
    Slcan,
    CandleLight,
    LogicDecode {
        decoder: LogicDecoderConfig,
    },
    /// 诊断协议层 (ISO-TP / UDS / OBD-II / J1939)
    ///
    /// 注意:诊断流程走独立的 `DiagnosticEngine` + `BridgeCanBackend` 管线,
    /// 不通过 `ProtocolEngine` 的 feed/encode 通路。`create_engine` 对此变体
    /// 返回 `RawDataEngine` 占位,真正的诊断 dispatch 在 `state.rs` 中实现。
    Diagnostic {
        config: DiagnosticConfig,
    },
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self::JustFloat { channels: Some(4) }
    }
}
