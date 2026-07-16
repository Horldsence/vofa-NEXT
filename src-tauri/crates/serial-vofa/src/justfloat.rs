use serial_core::DataFrame;
use std::io::Cursor;

use crate::engine::ProtocolEngine;

/// JustFloat 协议引擎
///
/// 数据格式: N × 4字节小端浮点 + 帧尾 [0x00, 0x00, 0x80, 0x7f]
/// 帧尾是小端 +Infinity 的字节表示, 用作同步标记
pub struct JustFloatEngine {
    channels: usize,
    buf: Vec<u8>,
}

/// JustFloat 帧尾: 0x00 0x00 0x80 0x7f (LE +Inf)
const TAIL: [u8; 4] = [0x00, 0x00, 0x80, 0x7f];

impl JustFloatEngine {
    pub fn new(channels: usize) -> Self {
        Self {
            channels: channels.max(1),
            buf: Vec::with_capacity(1024),
        }
    }

    /// 在缓冲区中搜索帧尾, 返回帧尾起始位置
    fn find_tail(&self) -> Option<usize> {
        if self.buf.len() < TAIL.len() {
            return None;
        }
        for i in 0..=self.buf.len() - TAIL.len() {
            if self.buf[i..i + TAIL.len()] == TAIL {
                return Some(i);
            }
        }
        None
    }
}

impl ProtocolEngine for JustFloatEngine {
    fn feed(&mut self, data: &[u8]) -> Vec<DataFrame> {
        self.buf.extend_from_slice(data);
        let mut frames = Vec::new();

        while let Some(tail_pos) = self.find_tail() {
            // 帧尾之前的数据应为 4 的倍数
            let payload_len = tail_pos;
            if payload_len == 0 || payload_len % 4 != 0 {
                // 跳过无效数据
                self.buf.drain(..(tail_pos + TAIL.len()));
                continue;
            }

            let count = payload_len / 4;
            let mut channels = Vec::with_capacity(count);
            let mut cursor = Cursor::new(&self.buf[..payload_len]);
            for _ in 0..count {
                let mut bytes = [0u8; 4];
                use std::io::Read;
                if cursor.read_exact(&mut bytes).is_ok() {
                    channels.push(f32::from_le_bytes(bytes));
                }
            }

            if !channels.is_empty() {
                frames.push(DataFrame::new(channels));
            }

            // 移除已处理数据
            self.buf.drain(..(tail_pos + TAIL.len()));
        }

        // 防止缓冲区无限增长
        if self.buf.len() > 8192 {
            let drop = self.buf.len() - 4096;
            self.buf.drain(..drop);
        }

        frames
    }

    fn encode_channel(&mut self, channel: usize, value: f32) -> Vec<u8> {
        // 发送单通道: 构造完整帧 (该通道数据 + 其他通道为0 + 帧尾)
        let mut buf = Vec::with_capacity(self.channels * 4 + TAIL.len());
        for i in 0..self.channels {
            let v = if i == channel { value } else { 0.0 };
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf.extend_from_slice(&TAIL);
        buf
    }

    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(values.len() * 4 + TAIL.len());
        for &v in values {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf.extend_from_slice(&TAIL);
        buf
    }

    fn name(&self) -> &str {
        "JustFloat"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_justfloat() {
        let mut engine = JustFloatEngine::new(2);
        let mut data = Vec::new();
        data.extend_from_slice(&1.0_f32.to_le_bytes());
        data.extend_from_slice(&2.0_f32.to_le_bytes());
        data.extend_from_slice(&TAIL);

        let frames = engine.feed(&data);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![1.0, 2.0]);
    }

    #[test]
    fn test_parse_partial() {
        let mut engine = JustFloatEngine::new(1);
        let mut data = 1.5_f32.to_le_bytes().to_vec();
        data.extend_from_slice(&TAIL);

        // 分两次喂入
        let frames1 = engine.feed(&data[..3]);
        assert!(frames1.is_empty());
        let frames2 = engine.feed(&data[3..]);
        assert_eq!(frames2.len(), 1);
        assert_eq!(frames2[0].channels, vec![1.5]);
    }
}
