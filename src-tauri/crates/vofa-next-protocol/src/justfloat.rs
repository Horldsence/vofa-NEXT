use std::io::Cursor;
use vofa_next_core::DataFrame;

use crate::engine::ProtocolEngine;

/// JustFloat 协议引擎
///
/// 数据格式: N × 4字节小端浮点 + 帧尾 [0x00, 0x00, 0x80, 0x7f]
/// 帧尾是小端 +Infinity 的字节表示, 用作同步标记
///
/// channels:
/// - Some(n): 手动指定通道数, 编码时按该通道数生成
/// - None: 自动检测模式, 由首帧 payload_len / 4 推断通道数
pub struct JustFloatEngine {
    /// 配置的通道数 (None 表示自动检测)
    channels: Option<usize>,
    /// 自动模式检测到的通道数 (仅在自动模式下使用)
    detected: Option<usize>,
    buf: Vec<u8>,
}

/// JustFloat 帧尾: 0x00 0x00 0x80 0x7f (LE +Inf)
const TAIL: [u8; 4] = [0x00, 0x00, 0x80, 0x7f];

impl JustFloatEngine {
    pub fn new(channels: Option<usize>) -> Self {
        Self {
            channels,
            detected: None,
            buf: Vec::with_capacity(1024),
        }
    }

    /// 当前生效通道数 (优先自动检测结果, 其次配置值, 默认 1)
    fn effective_channels(&self) -> usize {
        self.detected.or(self.channels).unwrap_or(1).max(1)
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

            // 自动检测模式: 由首帧推断通道数
            if self.channels.is_none() && self.detected.is_none() {
                self.detected = Some(count);
            }

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
        let n = self.effective_channels();
        let mut buf = Vec::with_capacity(n * 4 + TAIL.len());
        for i in 0..n {
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

    fn detected_channels(&self) -> Option<usize> {
        // 仅在自动模式且已检测到时返回
        if self.channels.is_none() {
            self.detected
        } else {
            None
        }
    }

    fn is_auto_mode(&self) -> bool {
        self.channels.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_justfloat() {
        let mut engine = JustFloatEngine::new(Some(2));
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
        let mut engine = JustFloatEngine::new(Some(1));
        let mut data = 1.5_f32.to_le_bytes().to_vec();
        data.extend_from_slice(&TAIL);

        // 分两次喂入
        let frames1 = engine.feed(&data[..3]);
        assert!(frames1.is_empty());
        let frames2 = engine.feed(&data[3..]);
        assert_eq!(frames2.len(), 1);
        assert_eq!(frames2[0].channels, vec![1.5]);
    }

    #[test]
    fn test_auto_mode_detect_channels() {
        // 自动模式: 由首帧 payload_len / 4 推断
        let mut engine = JustFloatEngine::new(None);
        assert!(engine.is_auto_mode());
        assert_eq!(engine.detected_channels(), None);

        let mut data = Vec::new();
        data.extend_from_slice(&10.0_f32.to_le_bytes());
        data.extend_from_slice(&20.0_f32.to_le_bytes());
        data.extend_from_slice(&30.0_f32.to_le_bytes());
        data.extend_from_slice(&TAIL);

        let frames = engine.feed(&data);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![10.0, 20.0, 30.0]);
        // 检测到 3 通道
        assert_eq!(engine.detected_channels(), Some(3));
    }

    #[test]
    fn test_manual_mode_not_auto() {
        let engine = JustFloatEngine::new(Some(4));
        assert!(!engine.is_auto_mode());
        assert_eq!(engine.detected_channels(), None);
    }

    #[test]
    fn test_auto_mode_multi_frames() {
        // 自动模式多帧
        let mut engine = JustFloatEngine::new(None);
        let mut data = Vec::new();
        // 第一帧 2 通道
        data.extend_from_slice(&1.0_f32.to_le_bytes());
        data.extend_from_slice(&2.0_f32.to_le_bytes());
        data.extend_from_slice(&TAIL);
        // 第二帧 2 通道
        data.extend_from_slice(&3.0_f32.to_le_bytes());
        data.extend_from_slice(&4.0_f32.to_le_bytes());
        data.extend_from_slice(&TAIL);

        let frames = engine.feed(&data);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].channels, vec![1.0, 2.0]);
        assert_eq!(frames[1].channels, vec![3.0, 4.0]);
        assert_eq!(engine.detected_channels(), Some(2));
    }

    #[test]
    fn test_encode_uses_detected_channels() {
        // 自动模式: 检测后编码使用检测到的通道数
        let mut engine = JustFloatEngine::new(None);
        let mut data = Vec::new();
        data.extend_from_slice(&1.0_f32.to_le_bytes());
        data.extend_from_slice(&2.0_f32.to_le_bytes());
        data.extend_from_slice(&TAIL);
        let _ = engine.feed(&data);
        assert_eq!(engine.detected_channels(), Some(2));

        // 编码单通道 0 = 5.0, 应生成 2 通道帧 (5.0, 0.0) + TAIL
        let encoded = engine.encode_channel(0, 5.0);
        assert_eq!(encoded.len(), 2 * 4 + TAIL.len());
        assert_eq!(&encoded[8..], &TAIL);
        assert_eq!(f32::from_le_bytes(encoded[0..4].try_into().unwrap()), 5.0);
        assert_eq!(f32::from_le_bytes(encoded[4..8].try_into().unwrap()), 0.0);
    }
}
