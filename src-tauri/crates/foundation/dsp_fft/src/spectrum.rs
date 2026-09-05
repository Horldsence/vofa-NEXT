//! Sample-driven FFT; display consumers receive only the most recent spectrum.
use crate::{SpectrumFrame, StreamingFft, TransformConfig, TransformError};
pub use dsp_window::WindowType;
use serde::{Deserialize, Serialize};

/// 频谱输出模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SpectrumOutput {
    /// 振幅谱 |X(k)| / N
    #[default]
    Magnitude,
    /// 功率谱 |X(k)|^2 / N^2
    Power,
    /// 功率谱密度 |X(k)|^2 / (N * fs * cg^2), cg=窗相干增益
    PSD,
    /// 10 * log10(Power + eps), 单位 dB
    Decibel,
}

/// 频谱计算结果 — 一组 (频率, 值) 配对
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectrumResult {
    /// 频率 (Hz), 长度 = window_size / 2 + 1
    pub frequencies: Vec<f32>,
    /// 频谱值 (Magnitude/Power/PSD/Decibel), 与 frequencies 对齐
    pub values: Vec<f32>,
}

pub struct SpectrumAnalyzer {
    config: TransformConfig,
    output: SpectrumOutput,
    stream: StreamingFft,
    latest: Option<SpectrumFrame>,
    samples: usize,
    epoch: u64,
    #[cfg(test)]
    frequencies: Vec<f32>,
}

impl SpectrumAnalyzer {
    pub fn new(n: usize, window: WindowType, output: SpectrumOutput, sample_rate: f32) -> Self {
        Self::with_config(
            TransformConfig {
                window_size: n.max(2),
                hop_size: (n / 2).max(1),
                window_type: window,
                sample_rate,
            },
            output,
        )
        .expect("validated FFT configuration")
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn with_config(
        config: TransformConfig,
        output: SpectrumOutput,
    ) -> Result<Self, TransformError> {
        let stream = StreamingFft::new(config, 0)?;
        Ok(Self {
            config,
            output,
            stream,
            latest: None,
            samples: 0,
            epoch: 0,
            #[cfg(test)]
            frequencies: (0..=config.window_size / 2)
                .map(|k| k as f32 * config.sample_rate / config.window_size as f32)
                .collect(),
        })
    }

    pub fn push(&mut self, value: f32) {
        self.push_with(value, |_| {});
    }

    /// Consumers run synchronously for each completed frame; nothing queues behind UI refresh.
    pub fn push_with(&mut self, value: f32, mut emit: impl FnMut(&SpectrumFrame)) {
        self.samples = self.samples.saturating_add(1);
        let latest = &mut self.latest;
        self.stream
            .push(&[value], |frame| {
                emit(&frame);
                *latest = Some(frame);
            })
            .expect("analyser stream is open");
    }

    pub fn push_slice(&mut self, values: &[f32]) {
        for &value in values {
            self.push(value);
        }
    }

    pub const fn is_ready(&self) -> bool {
        self.samples >= self.config.window_size
    }
    pub const fn window_size(&self) -> usize {
        self.config.window_size
    }
    pub const fn hop_size(&self) -> usize {
        self.config.hop_size
    }
    pub const fn sample_rate(&self) -> f32 {
        self.config.sample_rate
    }
    pub const fn window_type(&self) -> WindowType {
        self.config.window_type
    }
    pub const fn output(&self) -> SpectrumOutput {
        self.output
    }

    pub fn compute(&mut self) -> Option<SpectrumResult> {
        if !self.is_ready() {
            return None;
        }
        self.latest.take().map(|frame| frame.display(self.output))
    }

    pub const fn set_output(&mut self, output: SpectrumOutput) {
        self.output = output;
    }

    pub fn set_window_type(&mut self, window_type: WindowType) {
        self.config.window_type = window_type;
        self.stream = StreamingFft::new(self.config, self.epoch + 1).expect("valid window overlap");
        self.reset();
    }

    pub fn reset(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.stream.reset(self.epoch);
        self.samples = 0;
        self.latest = None;
    }
}

#[cfg(test)]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap
)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn test_push_and_ready() {
        let mut analyzer =
            SpectrumAnalyzer::new(8, WindowType::Rect, SpectrumOutput::Magnitude, 1000.0);
        assert!(!analyzer.is_ready());
        for i in 0..8 {
            analyzer.push(i as f32);
        }
        assert!(analyzer.is_ready());
    }

    #[test]
    fn test_compute_not_ready() {
        let mut analyzer =
            SpectrumAnalyzer::new(8, WindowType::Rect, SpectrumOutput::Magnitude, 1000.0);
        analyzer.push(1.0);
        assert!(analyzer.compute().is_none());
    }

    #[test]
    fn test_fft_sine_signal_peak() {
        // 采样率 1000 Hz, 窗口 256 点, 信号频率 50 Hz
        // FFT 后应在 bin k=50*256/1000=12.8≈13 处出现峰值
        let n = 256;
        let fs = 1000.0;
        let freq = 50.0;
        let mut analyzer =
            SpectrumAnalyzer::new(n, WindowType::Rect, SpectrumOutput::Magnitude, fs);
        for i in 0..n {
            let t = i as f32 / fs;
            analyzer.push((2.0 * PI * freq * t).sin());
        }
        let result = analyzer.compute().expect("应能计算");
        assert_eq!(result.frequencies.len(), n / 2 + 1);
        assert_eq!(result.values.len(), n / 2 + 1);

        // 找到峰值 bin
        let max_idx = result
            .values
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        // 峰值频率应接近 50 Hz (允许 ±1 bin)
        let peak_freq = result.frequencies[max_idx];
        assert!(
            (peak_freq - freq).abs() < fs / n as f32 * 2.0,
            "峰值频率 {peak_freq} 应接近 {freq}"
        );
    }

    #[test]
    fn test_fft_dc_signal() {
        // 直流信号 (常数) → bin 0 应为最大
        let n = 64;
        let mut analyzer =
            SpectrumAnalyzer::new(n, WindowType::Rect, SpectrumOutput::Magnitude, 1000.0);
        for _ in 0..n {
            analyzer.push(1.0);
        }
        let result = analyzer.compute().expect("应能计算");
        // bin 0 (DC) 应为最大
        let dc = result.values[0];
        for v in &result.values[1..] {
            assert!(dc > *v, "DC 分量应大于其他 bin");
        }
        // DC 分量 ≈ 1.0 (Rect 窗, 振幅 = |sum| / N = N/N = 1)
        assert!((dc - 1.0).abs() < 0.01, "DC 分量应接近 1.0, 实际 {dc}");
    }

    #[test]
    fn test_windowed_fft_reduces_leakage() {
        // 加窗后频谱泄漏应减少 (旁瓣降低)
        let n = 256;
        let fs = 1000.0;
        let freq = 50.5; // 非整数 bin 频率, 触发泄漏
        let signal: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * freq * i as f32 / fs).sin())
            .collect();

        // Rect 窗 (不加窗)
        let mut a_rect = SpectrumAnalyzer::new(n, WindowType::Rect, SpectrumOutput::Magnitude, fs);
        a_rect.push_slice(&signal);
        let r_rect = a_rect.compute().unwrap();

        // Hann 窗
        let mut a_hann = SpectrumAnalyzer::new(n, WindowType::Hann, SpectrumOutput::Magnitude, fs);
        a_hann.push_slice(&signal);
        let r_hann = a_hann.compute().unwrap();

        // 找到 Rect 窗的峰值
        let peak_idx = r_rect
            .values
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();

        // 测量远离峰值的旁瓣最大值 (取距离峰值 5 bin 以外的最大值)
        let side_rect = r_rect
            .values
            .iter()
            .enumerate()
            .filter(|(i, _)| (*i as i32 - peak_idx as i32).abs() > 5)
            .map(|(_, v)| *v)
            .fold(0.0f32, f32::max);
        let side_hann = r_hann
            .values
            .iter()
            .enumerate()
            .filter(|(i, _)| (*i as i32 - peak_idx as i32).abs() > 5)
            .map(|(_, v)| *v)
            .fold(0.0f32, f32::max);

        // Hann 窗的旁瓣应明显低于 Rect 窗
        assert!(
            side_hann < side_rect * 0.5,
            "Hann 旁瓣 {} 应小于 Rect 旁瓣 * 0.5 = {}",
            side_hann,
            side_rect * 0.5
        );
    }

    #[test]
    fn test_psd_output() {
        // PSD 输出不应为 NaN/Inf
        let n = 128;
        let mut analyzer = SpectrumAnalyzer::new(n, WindowType::Hann, SpectrumOutput::PSD, 1000.0);
        for i in 0..n {
            analyzer.push((i as f32 * 0.1).sin());
        }
        let result = analyzer.compute().expect("应能计算");
        for v in &result.values {
            assert!(v.is_finite(), "PSD 值应为有限数");
        }
    }

    #[test]
    fn test_decibel_output() {
        let n = 128;
        let mut analyzer =
            SpectrumAnalyzer::new(n, WindowType::Hann, SpectrumOutput::Decibel, 1000.0);
        for i in 0..n {
            analyzer.push((i as f32 * 0.1).sin());
        }
        let result = analyzer.compute().expect("应能计算");
        // dB 值应为有限数 (可能为负)
        for v in &result.values {
            assert!(v.is_finite(), "dB 值应为有限数");
        }
    }

    #[test]
    fn test_frequencies_correct() {
        let n = 8;
        let fs = 1000.0;
        let analyzer = SpectrumAnalyzer::new(n, WindowType::Rect, SpectrumOutput::Magnitude, fs);
        // 频率应为 [0, 125, 250, 375, 500] (n/2+1 = 5 个)
        assert_eq!(analyzer.frequencies.len(), 5);
        assert!((analyzer.frequencies[0] - 0.0).abs() < 1e-6);
        assert!((analyzer.frequencies[1] - 125.0).abs() < 1e-3);
        assert!((analyzer.frequencies[4] - 500.0).abs() < 1e-3);
    }

    #[test]
    fn test_reset() {
        let mut analyzer =
            SpectrumAnalyzer::new(8, WindowType::Rect, SpectrumOutput::Magnitude, 1000.0);
        for i in 0..8 {
            analyzer.push(i as f32);
        }
        assert!(analyzer.is_ready());
        analyzer.reset();
        assert!(!analyzer.is_ready());
    }
}
