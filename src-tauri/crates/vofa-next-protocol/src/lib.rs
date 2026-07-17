pub mod engine;
pub mod firewater;
pub mod justfloat;
pub mod rawdata;

pub use engine::ProtocolEngine;
pub use firewater::FireWaterEngine;
pub use justfloat::JustFloatEngine;
pub use rawdata::RawDataEngine;

use vofa_next_core::ProtocolConfig;

/// 根据配置创建协议引擎
pub fn create_engine(config: &ProtocolConfig) -> Box<dyn ProtocolEngine> {
    match config {
        ProtocolConfig::JustFloat { channels } => Box::new(JustFloatEngine::new(*channels)),
        ProtocolConfig::FireWater { channels } => Box::new(FireWaterEngine::new(*channels)),
        ProtocolConfig::RawData => Box::new(RawDataEngine::new()),
    }
}
