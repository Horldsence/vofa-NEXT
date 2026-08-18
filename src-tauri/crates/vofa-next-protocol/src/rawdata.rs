use crate::engine::{FeedOutput, ProtocolEngine};

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
    fn feed(&mut self, _data: &[u8]) -> FeedOutput {
        // RawData 不产生结构化数据帧
        FeedOutput::default()
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

    fn encode_frame(&mut self, _frame: &vofa_next_core::DataFrame) -> Vec<u8> {
        // RawData 无帧概念, DataFrame 重编码语义不符
        Vec::new()
    }

    fn new_worker(&self) -> Box<dyn ProtocolEngine> {
        Box::new(RawDataEngine::new())
    }
}

impl Default for RawDataEngine {
    fn default() -> Self {
        Self::new()
    }
}
