//! `ProtocolEngine for SchemaEngine` 实现

use protocol_engine::{FeedOutput, ProtocolEngine};
use vofa_core::DataFrame;

use crate::engine::{ParseAttempt, SchemaEngine};

/// 空符号 — 仅确保 protocol_impl 模块被 linker 保留 (供 integration tests 引用)
pub const fn _ensure_protocol_impl_used() {}

impl ProtocolEngine for SchemaEngine {
    fn feed(&mut self, data: &[u8]) -> FeedOutput {
        // Samples 块: 整体委托逻辑解码引擎
        if let Some(logic) = &mut self.logic {
            return logic.feed(data);
        }

        self.buf.extend_from_slice(data);
        let header = self.header_bytes();
        let mut frames = Vec::new();
        // 批内所有帧共享一个时间戳 (每次 feed 只读一次时钟)
        let ts = vofa_core::now_us();
        let mut base = 0usize;

        loop {
            // 1. 帧定界: 定位 Header (无 Header 块则从当前位置起解析)
            let frame_start = if header.is_empty() {
                0
            } else {
                match self.buf[base..]
                    .windows(header.len())
                    .position(|w| w == header.as_slice())
                {
                    Some(pos) => {
                        base += pos;
                        header.len()
                    }
                    None => {
                        // 未找到 header: 保留末尾 header.len()-1 字节 (跨包截断)
                        let keep = header.len().saturating_sub(1);
                        base = base.max(self.buf.len().saturating_sub(keep));
                        break;
                    }
                }
            };

            // 2. 按 decode 块求值
            match self.try_parse(&self.buf[base..], frame_start) {
                ParseAttempt::Incomplete => break,
                ParseAttempt::Invalid => {
                    // 假同步: 丢弃到本帧头之后, 重新同步
                    base += frame_start.max(1);
                }
                ParseAttempt::Done {
                    outputs,
                    valid,
                    consumed,
                } => {
                    base += consumed;
                    if valid {
                        // checksum 失败跳过该帧; 输出按端口序
                        let channels = self
                            .ports
                            .iter()
                            .map(|p| outputs.get(p).copied().unwrap_or(0.0))
                            .collect();
                        frames.push(DataFrame::with_timestamp(ts, channels));
                    }
                }
            }
        }

        if base > 0 {
            self.buf.drain(..base);
        }
        // 防止缓冲区无限增长 (与 JustFloatEngine 一致)
        if self.buf.len() > 8192 {
            let drop = self.buf.len() - 4096;
            self.buf.drain(..drop);
        }

        FeedOutput::from_frames(frames)
    }

    fn encode_channel(&mut self, channel: usize, value: f32) -> Vec<u8> {
        // 单通道发送: 该通道写值, 其余通道 0 (与 legacy 引擎语义一致)
        let mut values = vec![0.0f32; self.ports.len().max(channel + 1)];
        if channel < values.len() {
            values[channel] = value;
        }
        self.encode_channels(&values)
    }

    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8> {
        match &self.schema.encode {
            Some(blocks) => schema_types::encode_by_blocks(blocks, &self.ports, values),
            // Custom schema 未定义 encode 块: 无编码约定, 返回空
            None => Vec::new(),
        }
    }

    fn name(&self) -> &str {
        "CustomSchema"
    }

    fn take_pending(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf)
    }

    fn new_worker(&self) -> Box<dyn ProtocolEngine> {
        Box::new(SchemaEngine::new(self.schema.clone()))
    }
}