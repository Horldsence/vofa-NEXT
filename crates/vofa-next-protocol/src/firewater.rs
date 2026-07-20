use vofa_next_core::DataFrame;

use crate::engine::ProtocolEngine;

/// FireWater 协议引擎
///
/// 数据格式: ASCII 逗号分隔浮点 + 换行
/// 示例: "1.23,4.56,7.89\n"
///
/// channels:
/// - Some(n): 手动指定通道数 (用于编码)
/// - None: 自动检测模式, 由首行字段数推断通道数
pub struct FireWaterEngine {
    /// 配置的通道数 (None 表示自动检测)
    channels: Option<usize>,
    /// 自动模式检测到的通道数
    detected: Option<usize>,
    buf: String,
}

impl FireWaterEngine {
    pub fn new(channels: Option<usize>) -> Self {
        Self {
            channels,
            detected: None,
            buf: String::with_capacity(1024),
        }
    }
}

impl ProtocolEngine for FireWaterEngine {
    fn feed(&mut self, data: &[u8]) -> Vec<DataFrame> {
        // 追加数据到缓冲区
        if let Ok(s) = std::str::from_utf8(data) {
            self.buf.push_str(s);
        } else {
            // 非 UTF-8 数据, 丢弃
            return Vec::new();
        }

        let mut frames = Vec::new();

        while let Some(pos) = self.buf.find('\n') {
            let line = self.buf[..pos].trim_matches('\r');
            if !line.is_empty() {
                let channels: Vec<f32> = line
                    .split(',')
                    .filter_map(|s| s.trim().parse::<f32>().ok())
                    .collect();

                if !channels.is_empty() {
                    // 自动检测模式: 由首行字段数推断通道数
                    if self.channels.is_none() && self.detected.is_none() {
                        self.detected = Some(channels.len());
                    }
                    frames.push(DataFrame::new(channels));
                }
            }
            self.buf.drain(..=pos);
        }

        // 防止缓冲区无限增长 (无换行的超长行)
        if self.buf.len() > 8192 {
            self.buf.clear();
        }

        frames
    }

    fn encode_channel(&mut self, _channel: usize, value: f32) -> Vec<u8> {
        // 单通道编码: 仅发送该通道值 (其他通道不发送, 避免误用)
        // 注: FireWater 协议中, 单通道编码无标准做法, 这里采用与 encode_channels
        // 一致的行为 — 仅发一个值
        format!("{:.6}\n", value).into_bytes()
    }

    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8> {
        let s: Vec<String> = values.iter().map(|v| format!("{:.6}", v)).collect();
        format!("{}\n", s.join(",")).into_bytes()
    }

    fn name(&self) -> &str {
        "FireWater"
    }

    fn detected_channels(&self) -> Option<usize> {
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
    fn test_parse_firewater() {
        let mut engine = FireWaterEngine::new(Some(3));
        let frames = engine.feed(b"1.0,2.0,3.0\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_parse_multiple_lines() {
        let mut engine = FireWaterEngine::new(Some(2));
        let frames = engine.feed(b"1.0,2.0\n3.0,4.0\n");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].channels, vec![1.0, 2.0]);
        assert_eq!(frames[1].channels, vec![3.0, 4.0]);
    }

    #[test]
    fn test_auto_mode_detect_channels() {
        let mut engine = FireWaterEngine::new(None);
        assert!(engine.is_auto_mode());
        assert_eq!(engine.detected_channels(), None);

        let frames = engine.feed(b"1.0,2.0,3.0,4.0\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(engine.detected_channels(), Some(4));
    }

    #[test]
    fn test_manual_mode_not_auto() {
        let engine = FireWaterEngine::new(Some(4));
        assert!(!engine.is_auto_mode());
        assert_eq!(engine.detected_channels(), None);
    }

    #[test]
    fn test_auto_mode_multi_lines() {
        let mut engine = FireWaterEngine::new(None);
        let frames = engine.feed(b"1.0,2.0\n3.0,4.0\n");
        assert_eq!(frames.len(), 2);
        assert_eq!(engine.detected_channels(), Some(2));
    }

    #[test]
    fn test_partial_line_buffered() {
        let mut engine = FireWaterEngine::new(Some(2));
        let _ = engine.feed(b"1.0,2.");
        let frames = engine.feed(b"0\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![1.0, 2.0]);
    }
}
