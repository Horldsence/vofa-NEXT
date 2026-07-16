use serial_core::DataFrame;

use crate::engine::ProtocolEngine;

/// FireWater 协议引擎
///
/// 数据格式: ASCII 逗号分隔浮点 + 换行
/// 示例: "1.23,4.56,7.89\n"
pub struct FireWaterEngine {
    #[allow(dead_code)]
    channels: usize,
    buf: String,
}

impl FireWaterEngine {
    pub fn new(channels: usize) -> Self {
        Self {
            channels: channels.max(1),
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
        format!("{:.6}\n", value).into_bytes()
    }

    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8> {
        let s: Vec<String> = values.iter().map(|v| format!("{:.6}", v)).collect();
        format!("{}\n", s.join(",")).into_bytes()
    }

    fn name(&self) -> &str {
        "FireWater"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_firewater() {
        let mut engine = FireWaterEngine::new(3);
        let frames = engine.feed(b"1.0,2.0,3.0\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_parse_multiple_lines() {
        let mut engine = FireWaterEngine::new(2);
        let frames = engine.feed(b"1.0,2.0\n3.0,4.0\n");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].channels, vec![1.0, 2.0]);
        assert_eq!(frames[1].channels, vec![3.0, 4.0]);
    }
}
