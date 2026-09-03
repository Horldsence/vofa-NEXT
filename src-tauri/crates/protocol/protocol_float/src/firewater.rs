use protocol_engine::{split_at_boundaries, FeedOutput, ProtocolEngine};
use vofa_core::DataFrame;

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
    fn feed(&mut self, data: &[u8]) -> FeedOutput {
        // 追加数据到缓冲区
        if let Ok(s) = std::str::from_utf8(data) {
            self.buf.push_str(s);
        } else {
            // 非 UTF-8 数据, 丢弃
            return FeedOutput::default();
        }

        let mut frames = Vec::new();
        let mut start = 0usize;
        // 批内所有帧共享一个时间戳 (每次 feed 只读一次时钟, 避免每帧系统调用)
        let ts = vofa_core::now_us();

        // 单次遍历按行切分 (原实现每行 drain 一次, O(n²))
        while let Some(rel) = self.buf.as_bytes()[start..]
            .iter()
            .position(|&b| b == b'\n')
        {
            let pos = start + rel;
            let line = self.buf[start..pos].trim_matches('\r');
            start = pos + 1;
            if line.is_empty() {
                continue;
            }
            let channels: Vec<f32> = line
                .split(',')
                .filter_map(|s| s.trim().parse::<f32>().ok())
                .collect();

            if !channels.is_empty() {
                // 自动检测模式: 由首行字段数推断通道数
                if self.channels.is_none() && self.detected.is_none() {
                    self.detected = Some(channels.len());
                }
                frames.push(DataFrame::with_timestamp(ts, channels));
            }
        }

        // 一次性移除已处理前缀
        if start > 0 {
            self.buf.drain(..start);
        }
        // 防止缓冲区无限增长 (无换行的超长行)
        if self.buf.len() > 8192 {
            self.buf.clear();
        }

        FeedOutput::from_frames(frames)
    }

    fn encode_channel(&mut self, _channel: usize, value: f32) -> Vec<u8> {
        // 单通道编码: 仅发送该通道值 (其他通道不发送, 避免误用)
        // 注: FireWater 协议中, 单通道编码无标准做法, 这里采用与 encode_channels
        // 一致的行为 — 仅发一个值
        format!("{value:.6}\n").into_bytes()
    }

    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8> {
        let s: Vec<String> = values.iter().map(|v| format!("{v:.6}")).collect();
        format!("{}\n", s.join(",")).into_bytes()
    }

    fn encode_frame(&mut self, frame: &DataFrame) -> Vec<u8> {
        // 自动通道模式且尚未 detected (纯编码侧, 未 feed 过): 以输入帧通道数为准
        if self.channels.is_none() && self.detected.is_none() {
            self.detected = Some(frame.channels.len());
        }
        self.encode_channels(&frame.channels)
    }

    fn name(&self) -> &'static str {
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

    fn split_aligned(&self, data: &[u8], workers: usize) -> Option<Vec<std::ops::Range<usize>>> {
        // 边界 = 每个 \n 之后的位置 (ASCII 边界不会切断 UTF-8 多字节序列)
        let boundaries: Vec<usize> = data
            .iter()
            .enumerate()
            .filter(|(_, &b)| b == b'\n')
            .map(|(i, _)| i + 1)
            .collect();
        Some(split_at_boundaries(&boundaries, workers))
    }

    fn take_pending(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf).into_bytes()
    }

    fn new_worker(&self) -> Box<dyn ProtocolEngine> {
        Box::new(Self::new(self.channels))
    }
}
