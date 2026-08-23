//! 协议配置 — schema 的 legacy_config / TestDataLink.protocol 用。
//!
//! 仅保留 schema 所需的最小集 (ProtocolConfig);
//! 传输 / 控件 / 流水线配置由 `vofa_core::config` 承担。

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

impl ProtocolConfig {
    /// 手动指定的通道数 (仅 JustFloat/FireWater 有通道概念; None = 自动检测或无通道概念)
    pub const fn manual_channels(&self) -> Option<usize> {
        match self {
            Self::JustFloat { channels } | Self::FireWater { channels } => *channels,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_channels_only_for_float_protocols() {
        assert_eq!(
            ProtocolConfig::JustFloat { channels: Some(2) }.manual_channels(),
            Some(2)
        );
        assert_eq!(
            ProtocolConfig::FireWater { channels: Some(5) }.manual_channels(),
            Some(5)
        );
        assert_eq!(
            ProtocolConfig::JustFloat { channels: None }.manual_channels(),
            None
        );
        assert_eq!(ProtocolConfig::RawData.manual_channels(), None);
        assert_eq!(ProtocolConfig::Slcan.manual_channels(), None);
    }
}
