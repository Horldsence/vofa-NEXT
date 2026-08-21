//! 逆 FFT (IFFT) — 从振幅谱合成时域信号
//!
//! 与 [`crate::spectrum::SpectrumAnalyzer`] 的 `Magnitude` 输出 (|X(k)|/N) 对应:
//! 给定 N/2+1 个振幅谱 bin, 以零相位重建频谱 (纯实数), 再做实数逆 FFT。
//! 因 Magnitude 的 /N 缩放与 c2r 逆变换的未归一化相互抵消, 得到长度为 N 的
//! 对称(偶)时域信号。
//!
//! 注: 由于正向分析只保留振幅、丢弃相位, 重建结果是"具有相同振幅谱"的偶信号
//! (对余弦等偶信号可精确还原; 对含相位信息的信号则还原其零相位分量)。

use realfft::{ComplexToReal, RealFftPlanner};
use rustfft::num_complex::Complex32;
use std::sync::Arc;

/// 逆 FFT 合成器 — 缓存 c2r planner, 避免每次合成重新规划
pub struct IfftSynth {
    n: usize,
    c2r: Arc<dyn ComplexToReal<f32>>,
    input: Vec<Complex32>,
    output: Vec<f32>,
}

impl IfftSynth {
    /// 创建合成器
    ///
    /// - n: 时域信号长度 (与正向 FFT 的窗口大小一致, 建议 2 的幂)
    #[allow(clippy::cast_precision_loss)]
    pub fn new(n: usize) -> Self {
        let n = n.max(2);
        let mut planner = RealFftPlanner::<f32>::new();
        let c2r = planner.plan_fft_inverse(n);
        let input = c2r.make_input_vec();
        let output = c2r.make_output_vec();
        Self {
            n,
            c2r,
            input,
            output,
        }
    }

    /// 从振幅谱合成时域信号 (零相位, 偶数对称)
    ///
    /// `magnitudes`: N/2+1 个振幅谱 bin (与 SpectrumAnalyzer Magnitude 输出一致)。
    /// 返回长度为 N 的时域信号。
    pub fn synthesize(&mut self, magnitudes: &[f32]) -> Vec<f32> {
        for (i, slot) in self.input.iter_mut().enumerate() {
            let m = magnitudes.get(i).copied().unwrap_or(0.0);
            // Magnitude 输出 = |X(k)|/N, 而 c2r 为未归一化逆变换 (无 1/N),
            // 两者缩放抵消: x[n] = c2r(values)[n] 直接成立。
            // 零相位重建 → 频谱纯实数 (imag=0)。
            let re = if m.is_finite() { m } else { 0.0 };
            *slot = Complex32::new(re, 0.0);
        }
        // bin 0 与 bin N/2 已保证为实 (imag=0), 输出为实信号
        if self.c2r.process(&mut self.input, &mut self.output).is_ok() {
            self.output.clone()
        } else {
            vec![0.0; self.n]
        }
    }
}

/// IFFT 节点播放状态 — 缓存重建后的时域缓冲, 逐帧读出 (图编译热路径)
#[derive(Default)]
pub struct IfftState {
    /// 重建后的时域采样缓冲
    buffer: Vec<f32>,
    /// 下一个读出的采样下标 (环形播放)
    pos: usize,
    /// 缓存的合成器 (按窗口大小懒创建)
    synth: Option<IfftSynth>,
}

impl IfftState {
    /// 读取下一个采样 (环形播放; 空缓冲返回 0.0)
    pub fn next_sample(&mut self) -> f32 {
        if self.buffer.is_empty() {
            return 0.0;
        }
        let v = self.buffer[self.pos];
        self.pos = (self.pos + 1) % self.buffer.len();
        v
    }

    /// 用最新振幅谱重建缓冲并复位播放位置
    pub fn synth(&mut self, magnitudes: &[f32], n: usize) {
        let synth = self.synth.get_or_insert_with(|| IfftSynth::new(n));
        self.buffer = synth.synthesize(magnitudes);
        self.pos = 0;
    }

    /// 清空缓冲并复位播放位置 (无上游源时输出 0)
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.pos = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spectrum::SpectrumAnalyzer;
    use crate::spectrum::SpectrumOutput;
    use dsp_window::WindowType;
    use std::f32::consts::PI;

    #[test]
    fn test_ifft_reconstructs_even_signal() {
        // 余弦(偶)信号 + Rect 窗 + bin 对齐 → 振幅谱 → IFFT 应精确还原
        let n = 256;
        let fs = 1000.0;
        let k = 8; // bin 对齐频率
        let freq = k as f32 * fs / n as f32;
        let signal: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * freq * i as f32 / fs).cos())
            .collect();

        let mut analyzer =
            SpectrumAnalyzer::new(n, WindowType::Rect, SpectrumOutput::Magnitude, fs);
        analyzer.push_slice(&signal);
        let result = analyzer.compute().expect("应能计算");

        let mut synth = IfftSynth::new(n);
        let recon = synth.synthesize(&result.values);
        assert_eq!(recon.len(), n);

        for i in 0..n {
            let diff = (recon[i] - signal[i]).abs();
            assert!(
                diff < 1e-3,
                "i={i} recon={} sig={} diff={diff}",
                recon[i],
                signal[i]
            );
        }
    }

    #[test]
    fn test_ifft_dc_reconstruction() {
        // 直流信号 → 振幅谱 bin0=1 → IFFT 还原为常数 1
        let n = 64;
        let fs = 1000.0;
        let mut analyzer =
            SpectrumAnalyzer::new(n, WindowType::Rect, SpectrumOutput::Magnitude, fs);
        for _ in 0..n {
            analyzer.push(1.0);
        }
        let result = analyzer.compute().expect("应能计算");
        let mut synth = IfftSynth::new(n);
        let recon = synth.synthesize(&result.values);
        for v in &recon {
            assert!((v - 1.0).abs() < 1e-3, "直流重建应接近 1.0, 实际 {v}");
        }
    }

    #[test]
    fn test_ifft_state_playback() {
        let mut state = IfftState::default();
        // 空缓冲读出 0
        assert!((state.next_sample() - 0.0).abs() < 1e-6);
        // 合成一个简单缓冲
        let n = 8;
        let mut synth = IfftSynth::new(n);
        // 振幅谱: 仅 bin 0 = 1 (直流), 其余 0 → 重建为常数 1
        let magnitudes: Vec<f32> = {
            let mut v = vec![0.0; n / 2 + 1];
            v[0] = 1.0;
            v
        };
        state.buffer = synth.synthesize(&magnitudes);
        state.pos = 0;
        // 环形播放应持续读出 1.0
        for _ in 0..(n * 3) {
            assert!((state.next_sample() - 1.0).abs() < 1e-3);
        }
    }
}
