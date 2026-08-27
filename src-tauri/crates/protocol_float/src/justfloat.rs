use protocol_engine::{split_at_boundaries, FeedOutput, ProtocolEngine};
use vofa_core::DataFrame;

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
        let ts = vofa_core::now_us();

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

    fn encode_frame(&mut self, frame: &DataFrame) -> Vec<u8> {
        // 自动通道模式且尚未 detected (纯编码侧, 未 feed 过): 以输入帧通道数为准,
        // 避免 encode_channel 回退到默认 1 通道; 同时记下 detected, 后续编码保持一致
        if self.channels.is_none() && self.detected.is_none() {
            self.detected = Some(frame.channels.len());
        }
        self.encode_channels(&frame.channels)
    }

    fn name(&self) -> &'static str {
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
        Some(split_at_boundaries(&boundaries, workers))
    }

    fn take_pending(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf)
    }

    fn new_worker(&self) -> Box<dyn ProtocolEngine> {
        Box::new(Self::new(self.channels))
    }
}
