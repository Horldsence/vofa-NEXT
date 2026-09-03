//! schema → ProtocolEngine 编译入口

use protocol_engine::ProtocolEngine;
use schema_types::{ProtocolConfig, ProtocolSchema, SchemaPreset};

use crate::engine::SchemaEngine;

/// 预设对应的缺省 legacy 配置 (legacy_config 缺失时的兜底)
pub const fn default_legacy_config(preset: SchemaPreset) -> Option<ProtocolConfig> {
    match preset {
        SchemaPreset::JustFloat => Some(ProtocolConfig::JustFloat { channels: None }),
        SchemaPreset::FireWater => Some(ProtocolConfig::FireWater { channels: None }),
        SchemaPreset::RawData => Some(ProtocolConfig::RawData),
        SchemaPreset::Slcan => Some(ProtocolConfig::Slcan),
        SchemaPreset::CandleLight => Some(ProtocolConfig::CandleLight),
        // LogicDecode 需要具体解码器配置, 无合理缺省
        SchemaPreset::LogicDecode | SchemaPreset::Custom => None,
    }
}

/// 编译帧 schema 为协议引擎
///
/// - `preset != Custom`: 用 `legacy_config` (缺失时按预设兜底) 走现有
///   [`compile_schema`] 内部分发到 legacy 引擎 (JustFloat/FireWater/Slcan/CandleLight/RawData),
///   完整保留自动检测 / 并行 split / CAN / 逻辑事件能力;
/// - `Custom`: 构造 [`SchemaEngine`] (流式帧解码 + encode 块编码)。
///
/// 注: 旧实现通过 `create_engine` 派发; 当前 crate 已拆分为
/// 4 个子 crate (protocol_float / protocol_can_bridge / logic_decoder), 故本 crate
/// 只覆盖 Custom 路径。Preset 路径应由调用方 (app shell) 调度。
pub fn compile_schema(schema: &ProtocolSchema) -> Box<dyn ProtocolEngine> {
    if schema.preset != SchemaPreset::Custom {
        let config = schema
            .legacy_config
            .clone()
            .or_else(|| default_legacy_config(schema.preset));
        if let Some(config) = config {
            return compile_legacy(config);
        }
        // 无 legacy 配置可用 (如 LogicDecode 缺 decoder): 回落 SchemaEngine
    }
    Box::new(SchemaEngine::new(schema.clone()))
}

/// 按 `ProtocolConfig` 调度到对应子 crate 的协议引擎
///
/// 此函数保留作为 schema_engine 内部的预设路径支持; 若调用方已持有子 crate
/// 引擎, 可跳过该调度, 直接构造 `SchemaEngine`。
pub fn compile_legacy(config: ProtocolConfig) -> Box<dyn ProtocolEngine> {
    use protocol_can_bridge::{CandleEngine, RawDataEngine, SlcanEngine};
    use protocol_float::{FireWaterEngine, JustFloatEngine};
    match config {
        ProtocolConfig::JustFloat { channels } => Box::new(JustFloatEngine::new(channels)),
        ProtocolConfig::FireWater { channels } => Box::new(FireWaterEngine::new(channels)),
        ProtocolConfig::RawData => Box::new(RawDataEngine::new()),
        ProtocolConfig::Slcan => Box::new(SlcanEngine::new()),
        ProtocolConfig::CandleLight => Box::new(CandleEngine::new()),
        ProtocolConfig::LogicDecode { .. } => Box::new(RawDataEngine::new()),
        // Diagnostic 不走 ProtocolEngine 路径 (走独立 DiagnosticEngine), 此处回落到 RawData 占位
        ProtocolConfig::Diagnostic { .. } => Box::new(RawDataEngine::new()),
    }
}
