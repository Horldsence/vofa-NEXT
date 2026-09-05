//! Bounded, one-shot reconstruction state for the compiled IFFT kernel.

/// IFFT 节点播放状态 — 缓存重建后的时域缓冲, 逐帧读出 (图编译热路径)
#[derive(Default)]
pub struct IfftState {
    /// 重建后的时域采样缓冲
    buffer: Vec<f32>,
    /// 下一个读出的采样下标 (按序消费)
    pos: usize,
    reconstruction: Option<crate::StreamingIfft>,
    stream_config: Option<crate::TransformConfig>,
    epoch: Option<u64>,
}

impl IfftState {
    /// 读取下一个采样 (按序消费; 无新样本返回 0.0)
    pub fn next_sample(&mut self) -> f32 {
        if self.pos >= self.buffer.len() {
            return 0.0;
        }
        let v = self.buffer[self.pos];
        self.pos += 1;
        v
    }

    /// Reconstruct a complex block exactly once, keeping at most one hop buffered.
    pub fn accept(&mut self, frame: &crate::SpectrumFrame) -> Result<(), crate::TransformError> {
        if self.stream_config != Some(frame.config) || self.epoch != Some(frame.epoch) {
            self.reconstruction = Some(crate::StreamingIfft::new(frame.config)?);
            self.stream_config = Some(frame.config);
            self.epoch = Some(frame.epoch);
        }
        if self.pos < self.buffer.len() {
            return Err(crate::TransformError::Discontinuity);
        }
        self.buffer.clear();
        self.pos = 0;
        let buffer = &mut self.buffer;
        self.reconstruction
            .as_mut()
            .expect("reconstruction initialized")
            .process(frame, |_, sample| buffer.push(sample))
    }

    /// 清空缓冲并复位播放位置 (无上游源时输出 0)
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.pos = 0;
        self.reconstruction = None;
        self.stream_config = None;
        self.epoch = None;
    }
}
