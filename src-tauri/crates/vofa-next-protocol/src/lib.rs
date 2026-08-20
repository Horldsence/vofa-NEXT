//! VOFA-NEXT 协议栈 façade — 纯 re-export, 无业务逻辑
//!
//! 实际实现位于 5 个子 crate:
//! - [`protocol_engine`]: `ProtocolEngine` trait + 输入/输出容器 + 解析/切分自由函数
//! - [`protocol_float`]: `JustFloatEngine` / `FireWaterEngine`
//! - [`protocol_can_bridge`]: `SlcanEngine` / `CandleEngine` / `RawDataEngine` + 常量
//! - [`logic_decoder`]: `LogicDecoderEngine` (UART/I2C/SPI 解码)
//! - [`schema_engine`]: `SchemaEngine` + `compile_schema`

pub use logic_decoder::LogicDecoderEngine;
pub use protocol_can_bridge::{
    CandleEngine, RawDataEngine, SlcanEngine, CAND_CMD_RX, CAND_CMD_TX, CAND_FRAME_SIZE,
    CAND_ID_EFF, CAND_ID_MASK, CAND_ID_RTR,
};
pub use protocol_engine::{
    detect_format, parse_ascii, parse_hex, split_at_boundaries, FeedOutput, InputFormat,
    ParsedInput, ProtocolEngine,
};
pub use protocol_float::{FireWaterEngine, JustFloatEngine};
pub use schema_engine::{compile_schema, SchemaEngine};

use vofa_next_core::ProtocolConfig;

/// 根据配置创建协议引擎
///
/// 注意:`Diagnostic` 变体返回 `RawDataEngine` 占位 — 诊断流程走独立的
/// `DiagnosticEngine` + `BridgeCanBackend` 管线,不通过 `ProtocolEngine` 的
/// feed/encode 通路。真正的诊断 dispatch 在 `state.rs` 中实现 (后续 Phase)。
pub fn create_engine(config: &ProtocolConfig) -> Box<dyn ProtocolEngine> {
    use protocol_can_bridge::{CandleEngine as Candle, RawDataEngine as RawData, SlcanEngine as Slcan};
    use protocol_float::{FireWaterEngine as FireWater, JustFloatEngine as JustFloat};
    match config {
        ProtocolConfig::JustFloat { channels } => Box::new(JustFloat::new(*channels)),
        ProtocolConfig::FireWater { channels } => Box::new(FireWater::new(*channels)),
        ProtocolConfig::RawData => Box::new(RawData::new()),
        ProtocolConfig::Slcan => Box::new(Slcan::new()),
        ProtocolConfig::CandleLight => Box::new(Candle::new()),
        ProtocolConfig::LogicDecode { decoder } => Box::new(LogicDecoderEngine::new(decoder.clone())),
        ProtocolConfig::Diagnostic { .. } => Box::new(RawData::new()),
    }
}