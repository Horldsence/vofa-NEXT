//! 频谱分析 — 滑动窗口 + FFT + 多种输出模式
//!
//! 使用 `realfft` 库对实数信号进行 FFT, 返回前半部分频谱 (N/2+1 个 bin)。
//!
//! 输出模式:
//! - Magnitude: 振幅谱 |X(k)| / N
//! - Power: 功率谱 |X(k)|^2 / N^2
//! - PSD: 功率谱密度 |X(k)|^2 / (N * fs * cg^2), cg=窗相干增益
//! - Decibel: 10 * log10(Power + eps)

use serde::{Deserialize, Serialize};
use realfft::{RealFftPlanner, RealToComplex};
use rustfft::num_complex::Complex32;
use std::sync::Arc;

pub use crate::window::WindowType;

/// 频谱输出模式
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SpectrumOutput {
    /// 振幅谱 |X(k)| / N
    Magnitude,
    /// 功率谱 |X(k)|^2 / N^2
    Power,
    /// 功率谱密度 |X(k)|^2 / (N * fs * cg^2), cg=窗相干增益
    PSD,
    /// 10 * log10(Power + eps), 单位 dB
    Decibel,
}

impl Default for SpectrumOutput {
    fn default() -> Self {
        SpectrumOutput::Magnitude
    }
}

/// 频谱计算结果 — 一组 (频率, 值) 配对
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectrumResult {
    /// 频率 (Hz), 长度 = window_size / 2 + 1
    pub frequencies: Vec<f32>,
    /// 频谱值 (Magnitude/Power/PSD/Decibel), 与 frequencies 对齐
    pub values: Vec<f32>,
}

/// 频谱分析器 — 滑动窗口 + FFT
///
/// 使用方法:
/// 1. 每帧调用 `push(value)` 推入新样本
/// 2. 定期调用 `compute()` 计算 FFT (例如 30 FPS)
///
/// 内部维护一个长度为 window_size 的环形缓冲区,
/// 计算时取出全部数据, 应用窗函数, 做 FFT, 转换为指定输出模式。
pub struct SpectrumAnalyzer {
    window_size: usize,
    window_type: WindowType,
    output: SpectrumOutput,
    sample_rate: f32,
    /// 样本环形缓冲区 (长度 = window_size)
    buffer: Vec<f32>,
    /// 下一个写入位置 (覆盖式)
    write_pos: usize,
    /// 已积累的样本数 (>= window_size 后认为窗口已满)
    samples_count: usize,
    /// FFT planner (缓存, 避免每次重建)
    /// `RealFftPlanner::plan_fft_forward` 返回 `Arc<dyn RealToComplex<f32>>`
    r2c: Arc<dyn RealToComplex<f32>>,
    /// FFT 输入缓冲 (加窗后的数据)
    fft_input: Vec<f32>,
    /// FFT 输出缓冲 (复数频谱)
    fft_output: Vec<Complex32>,
    /// 预计算的频率轴
    frequencies: Vec<f32>,
}

impl SpectrumAnalyzer {
    /// 创建新分析器
    ///
    /// - window_size: FFT 窗口大小 (建议 2 的幂, 如 256/512/1024/2048)
    /// - sample_rate: 采样率 (Hz), 用于计算频率轴
    pub fn new(
        window_size: usize,
        window_type: WindowType,
        output: SpectrumOutput,
        sample_rate: f32,
    ) -> Self {
        let n = window_size.max(2);
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(n);
        let fft_input = r2c.make_input_vec();
        let fft_output = r2c.make_output_vec();
        let frequencies: Vec<f32> = (0..=n / 2)
            .map(|k| k as f32 * sample_rate / n as f32)
            .collect();
        Self {
            window_size: n,
            window_type,
            output,
            sample_rate,
            buffer: vec![0.0; n],
            write_pos: 0,
            samples_count: 0,
            r2c,
            fft_input,
            fft_output,
            frequencies,
        }
    }

    /// 推入一个样本 (覆盖最旧样本)
    pub fn push(&mut self, value: f32) {
        self.buffer[self.write_pos] = value;
        self.write_pos = (self.write_pos + 1) % self.window_size;
        if self.samples_count < self.window_size {
            self.samples_count += 1;
        }
    }

    /// 批量推入
    pub fn push_slice(&mut self, values: &[f32]) {
        for &v in values {
            self.push(v);
        }
    }

    /// 是否积累了足够样本 (>= window_size)
    pub fn is_ready(&self) -> bool {
        self.samples_count >= self.window_size
    }

    /// 当前窗口大小
    pub fn window_size(&self) -> usize {
        self.window_size
    }

    /// 采样率
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// 当前窗类型
    pub fn window_type(&self) -> WindowType {
        self.window_type
    }

    /// 当前输出模式
    pub fn output(&self) -> SpectrumOutput {
        self.output
    }

    /// 计算频谱
    ///
    /// 若样本不足 (未填满窗口), 返回 None。
    /// 否则取出窗口数据 (按时间顺序), 加窗, FFT, 转换为输出模式。
    pub fn compute(&mut self) -> Option<SpectrumResult> {
        if !self.is_ready() {
            return None;
        }

        // 从环形缓冲区读取数据 (时间顺序: 最旧→最新)
        // write_pos 指向下一个写入位置, 即最旧数据
        let start = self.write_pos;
        for i in 0..self.window_size {
            self.fft_input[i] = self.buffer[(start + i) % self.window_size];
        }

        // 应用窗函数 (in-place)
        crate::window::apply_window(&self.window_type, &mut self.fft_input);

        // FFT — process 会读取 fft_input, 写入 fft_output
        if self.r2c.process(&mut self.fft_input, &mut self.fft_output).is_err() {
            return None;
        }

        // 转换为输出模式
        let n = self.window_size as f32;
        let half_n = self.window_size / 2 + 1;
        let cg = self.window_type.coherent_gain(self.window_size);
        let cg_sq = cg * cg;
        let fs = self.sample_rate;
        let eps: f32 = 1e-12;

        let values: Vec<f32> = self.fft_output.iter().take(half_n).map(|c| {
            let mag = c.norm(); // |X(k)|
            let power = mag * mag;
            match self.output {
                SpectrumOutput::Magnitude => mag / n,
                SpectrumOutput::Power => power / (n * n),
                SpectrumOutput::PSD => power / (n * fs * cg_sq + eps),
                SpectrumOutput::Decibel => {
                    let p = power / (n * n);
                    10.0 * (p + eps).log10()
                }
            }
        }).collect();

        Some(SpectrumResult {
            frequencies: self.frequencies.clone(),
            values,
        })
    }

    /// 修改输出模式 (无需重建 FFT planner)
    pub fn set_output(&mut self, output: SpectrumOutput) {
        self.output = output;
    }

    /// 修改窗类型 (无需重建 FFT planner)
    pub fn set_window_type(&mut self, window_type: WindowType) {
        self.window_type = window_type;
    }

    /// 重置状态 (清空缓冲区)
    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
        self.samples_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn test_push_and_ready() {
        let mut analyzer = SpectrumAnalyzer::new(8, WindowType::Rect, SpectrumOutput::Magnitude, 1000.0);
        assert!(!analyzer.is_ready());
        for i in 0..8 {
            analyzer.push(i as f32);
        }
        assert!(analyzer.is_ready());
    }

    #[test]
    fn test_compute_not_ready() {
        let mut analyzer = SpectrumAnalyzer::new(8, WindowType::Rect, SpectrumOutput::Magnitude, 1000.0);
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
        let mut analyzer = SpectrumAnalyzer::new(n, WindowType::Rect, SpectrumOutput::Magnitude, fs);
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
            "峰值频率 {} 应接近 {}",
            peak_freq,
            freq
        );
    }

    #[test]
    fn test_fft_dc_signal() {
        // 直流信号 (常数) → bin 0 应为最大
        let n = 64;
        let mut analyzer = SpectrumAnalyzer::new(n, WindowType::Rect, SpectrumOutput::Magnitude, 1000.0);
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
        assert!((dc - 1.0).abs() < 0.01, "DC 分量应接近 1.0, 实际 {}", dc);
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
        let mut analyzer = SpectrumAnalyzer::new(n, WindowType::Hann, SpectrumOutput::Decibel, 1000.0);
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
        let mut analyzer = SpectrumAnalyzer::new(8, WindowType::Rect, SpectrumOutput::Magnitude, 1000.0);
        for i in 0..8 {
            analyzer.push(i as f32);
        }
        assert!(analyzer.is_ready());
        analyzer.reset();
        assert!(!analyzer.is_ready());
    }
}
