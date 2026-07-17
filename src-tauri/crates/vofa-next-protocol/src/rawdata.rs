use vofa_next_core::DataFrame;

use crate::engine::ProtocolEngine;

/// RawData 协议引擎 — 不解析, 仅透传
///
/// 接收的原始字节不产生 DataFrame, 由前端直接显示
pub struct RawDataEngine;

impl RawDataEngine {
    pub fn new() -> Self {
        Self
    }
}

impl ProtocolEngine for RawDataEngine {
    fn feed(&mut self, _data: &[u8]) -> Vec<DataFrame> {
        // RawData 不产生结构化数据帧
        Vec::new()
    }

    fn encode_channel(&mut self, _channel: usize, value: f32) -> Vec<u8> {
        format!("{:.6}\n", value).into_bytes()
    }

    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8> {
        let s: Vec<String> = values.iter().map(|v| format!("{:.6}", v)).collect();
        format!("{}\n", s.join(",")).into_bytes()
    }

    fn name(&self) -> &str {
        "RawData"
    }
}

impl Default for RawDataEngine {
    fn default() -> Self {
        Self::new()
    }
}
