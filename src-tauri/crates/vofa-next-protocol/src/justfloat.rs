use vofa_next_core::DataFrame;

use crate::engine::{FeedOutput, ProtocolEngine};

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

    /// 从 from 位置开始在缓冲区中搜索帧尾, 返回相对 from 的偏移
    fn find_tail_from(&self, from: usize) -> Option<usize> {
        if self.buf.len() < from + TAIL.len() {
            return None;
        }
        self.buf[from..].windows(TAIL.len()).position(|w| w == TAIL)
    }
}

impl ProtocolEngine for JustFloatEngine {
    fn feed(&mut self, data: &[u8]) -> FeedOutput {
        self.buf.extend_from_slice(data);
        let mut frames = Vec::new();
        let mut start = 0usize;
        // 批内所有帧共享一个时间戳 (每次 feed 只读一次时钟, 避免每帧系统调用)
        let ts = vofa_next_core::now_us();

        // 单次遍历: 从上次位置继续找帧尾 (原实现每帧从头扫描 + drain, O(n²),
        // 高码率合批后是 feed 段的主要开销)
        while let Some(rel) = self.find_tail_from(start) {
            let frame_start = start;
            let tail_pos = start + rel;
            let payload_len = tail_pos - frame_start;
            start = tail_pos + TAIL.len();

            // 帧尾之前的数据应为 4 的倍数
            if payload_len == 0 || !payload_len.is_multiple_of(4) {
                continue; // 跳过无效数据
            }
            let count = payload_len / 4;

            // 自动检测模式: 由首帧推断通道数
            if self.channels.is_none() && self.detected.is_none() {
                self.detected = Some(count);
            }

            let mut channels = Vec::with_capacity(count);
            for i in 0..count {
                let o = frame_start + i * 4;
                channels.push(f32::from_le_bytes(self.buf[o..o + 4].try_into().unwrap()));
            }
            if !channels.is_empty() {
                frames.push(DataFrame::with_timestamp(ts, channels));
            }
        }

        // 一次性移除已处理前缀
        if start > 0 {
            self.buf.drain(..start);
        }
        // 防止缓冲区无限增长
        if self.buf.len() > 8192 {
            let drop = self.buf.len() - 4096;
            self.buf.drain(..drop);
        }

        FeedOutput::from_frames(frames)
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

    fn split_aligned(&self, data: &[u8], workers: usize) -> Option<Vec<std::ops::Range<usize>>> {
        // 边界 = 每个帧尾 TAIL (00 00 80 7F) 之后的位置
        // (扫描语义与 find_tail_from 一致: payload 含 TAIL 的误判行为与顺序解析保持一致)
        let mut boundaries = Vec::new();
        let mut start = 0usize;
        while start + TAIL.len() <= data.len() {
            match data[start..].windows(TAIL.len()).position(|w| w == TAIL) {
                Some(rel) => {
                    let end = start + rel + TAIL.len();
                    boundaries.push(end);
                    start = end;
                }
                None => break,
            }
        }
        Some(crate::engine::split_at_boundaries(&boundaries, workers))
    }

    fn take_pending(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf)
    }

    fn new_worker(&self) -> Box<dyn ProtocolEngine> {
        Box::new(JustFloatEngine::new(self.channels))
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

        let frames = engine.feed(&data).frames;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![1.0, 2.0]);
    }

    #[test]
    fn test_parse_partial() {
        let mut engine = JustFloatEngine::new(Some(1));
        let mut data = 1.5_f32.to_le_bytes().to_vec();
        data.extend_from_slice(&TAIL);

        // 分两次喂入
        let frames1 = engine.feed(&data[..3]).frames;
        assert!(frames1.is_empty());
        let frames2 = engine.feed(&data[3..]).frames;
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

        let frames = engine.feed(&data).frames;
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

        let frames = engine.feed(&data).frames;
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

    /// 顺序/并行等价性: 多帧 + 垃圾前缀 + 半帧尾
    #[test]
    fn test_split_aligned_equivalence() {
        let mut data = Vec::new();
        // 垃圾前缀 (使首个 TAIL 前的 payload 长度非 4 倍数, 被跳过)
        data.extend_from_slice(&[0x11, 0x22]);
        // 帧 1: 因垃圾前缀被判定无效 (顺序/并行行为一致)
        data.extend_from_slice(&1.0_f32.to_le_bytes());
        data.extend_from_slice(&2.0_f32.to_le_bytes());
        data.extend_from_slice(&TAIL);
        // 帧 2: 2 通道, 有效
        data.extend_from_slice(&3.0_f32.to_le_bytes());
        data.extend_from_slice(&4.0_f32.to_le_bytes());
        data.extend_from_slice(&TAIL);
        // 半帧尾
        data.extend_from_slice(&5.0_f32.to_le_bytes());

        // 顺序解析全量
        let mut seq_engine = JustFloatEngine::new(Some(2));
        let seq_frames = seq_engine.feed(&data).frames;

        // 并行: split_aligned 切分 + 逐块 new_worker().feed + append 合并
        let ranges = seq_engine
            .split_aligned(&data, 3)
            .expect("justfloat 应支持并行切分");
        let tail_start = ranges.last().map(|r| r.end).unwrap_or(0);
        let mut merged = crate::engine::FeedOutput::default();
        let mut concat = Vec::new();
        for r in &ranges {
            let mut w = seq_engine.new_worker();
            merged.append(w.feed(&data[r.clone()]));
            concat.extend_from_slice(&data[r.clone()]);
        }
        concat.extend_from_slice(&data[tail_start..]);

        // concat(块) + tail == 原 data; 半帧尾留在 tail 中
        assert_eq!(concat, data);
        assert_eq!(&data[tail_start..], &5.0_f32.to_le_bytes());

        // 结果逐项相等 (忽略 timestamp): 仅帧 2 有效
        let norm = |f: &DataFrame| f.channels.clone();
        let seq_norm: Vec<_> = seq_frames.iter().map(norm).collect();
        let par_norm: Vec<_> = merged.frames.iter().map(norm).collect();
        assert_eq!(seq_norm, par_norm);
        assert_eq!(seq_frames.len(), 1);
        assert_eq!(seq_frames[0].channels, vec![3.0, 4.0]);
    }
}
