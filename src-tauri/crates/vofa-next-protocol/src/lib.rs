pub mod candle;
pub mod engine;
pub mod firewater;
pub mod justfloat;
pub mod logic_decoder;
pub mod rawdata;
pub mod slcan;

pub use candle::CandleEngine;
pub use engine::ProtocolEngine;
pub use firewater::FireWaterEngine;
pub use justfloat::JustFloatEngine;
pub use logic_decoder::LogicDecoderEngine;
pub use rawdata::RawDataEngine;
pub use slcan::SlcanEngine;

use vofa_next_core::ProtocolConfig;

/// 根据配置创建协议引擎
///
/// 注意:`Diagnostic` 变体返回 `RawDataEngine` 占位 — 诊断流程走独立的
/// `DiagnosticEngine` + `BridgeCanBackend` 管线,不通过 `ProtocolEngine` 的
/// feed/encode 通路。真正的诊断 dispatch 在 `state.rs` 中实现 (后续 Phase)。
pub fn create_engine(config: &ProtocolConfig) -> Box<dyn ProtocolEngine> {
    match config {
        ProtocolConfig::JustFloat { channels } => Box::new(JustFloatEngine::new(*channels)),
        ProtocolConfig::FireWater { channels } => Box::new(FireWaterEngine::new(*channels)),
        ProtocolConfig::RawData => Box::new(RawDataEngine::new()),
        ProtocolConfig::Slcan => Box::new(SlcanEngine::new()),
        ProtocolConfig::CandleLight => Box::new(CandleEngine::new()),
        ProtocolConfig::LogicDecode { decoder } => {
            Box::new(LogicDecoderEngine::new(decoder.clone()))
        }
        ProtocolConfig::Diagnostic { .. } => Box::new(RawDataEngine::new()),
    }
}
