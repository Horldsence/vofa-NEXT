//! Sample-clocked, phase-preserving STFT and weighted overlap-add reconstruction.
//! Display cadence never controls computation. Working storage is bounded by N.

use std::sync::Arc;

use dsp_window::{apply_window, WindowType};
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use rustfft::num_complex::Complex32;
use serde::{Deserialize, Serialize};

use crate::{SpectrumOutput, SpectrumResult};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct TransformConfig {
    pub window_size: usize,
    pub hop_size: usize,
    pub window_type: WindowType,
    pub sample_rate: f32,
}

impl TransformConfig {
    /// Validate both allocation bounds and nonzero overlap-add coverage.
    pub fn validate(self) -> Result<(), TransformError> {
        let n = self.window_size;
        if !(2..=1_048_576).contains(&n)
            || self.hop_size == 0
            || self.hop_size > n
            || !self.sample_rate.is_finite()
            || self.sample_rate <= 0.0
        {
            return Err(TransformError::InvalidConfig);
        }
        let window = self.window();
        let mut coverage = vec![0.0_f64; self.hop_size];
        for (i, &w) in window.iter().enumerate() {
            coverage[i % self.hop_size] += f64::from(w).powi(2);
        }
        if coverage.iter().any(|&v| v < 1e-10) {
            return Err(TransformError::UncoveredWindow);
        }
        Ok(())
    }

    fn window(self) -> Vec<f32> {
        let mut window = vec![1.0; self.window_size];
        apply_window(&self.window_type, &mut window);
        window
    }
}

/// The spectrum is unnormalised complex rFFT data, never a display magnitude.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectrumFrame {
    pub config: TransformConfig,
    pub epoch: u64,
    pub sequence: u64,
    /// Signed source sample index; negative indices represent startup padding.
    pub start_sample: i64,
    /// Number of real input samples seen, excluding startup/end padding.
    pub valid_samples: u64,
    pub bins: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformError {
    InvalidConfig,
    UncoveredWindow,
    InvalidSpectrum,
    Discontinuity,
    Finished,
}

impl std::fmt::Display for TransformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidConfig => "invalid FFT size, hop, or sample rate",
            Self::UncoveredWindow => "window and hop leave samples without reconstruction coverage",
            Self::InvalidSpectrum => "invalid complex spectrum",
            Self::Discontinuity => "spectrum sequence or configuration changed; reset the stream",
            Self::Finished => "FFT stream is finished; reset before adding samples",
        })
    }
}

impl std::error::Error for TransformError {}

/// A bounded streaming analyser. Consume each emitted frame before producing more.
pub struct StreamingFft {
    config: TransformConfig,
    transform: Arc<dyn RealToComplex<f32>>,
    ring: Vec<f32>,
    input: Vec<f32>,
    output: Vec<Complex32>,
    scratch: Vec<Complex32>,
    window: Vec<f32>,
    write: usize,
    pending: usize,
    samples: u64,
    sequence: u64,
    epoch: u64,
    finished: bool,
}

impl StreamingFft {
    pub fn new(config: TransformConfig, epoch: u64) -> Result<Self, TransformError> {
        config.validate()?;
        let transform = RealFftPlanner::new().plan_fft_forward(config.window_size);
        Ok(Self {
            input: transform.make_input_vec(),
            output: transform.make_output_vec(),
            scratch: transform.make_scratch_vec(),
            ring: vec![0.0; config.window_size],
            window: config.window(),
            transform,
            config,
            write: config.window_size - config.hop_size,
            pending: 0,
            samples: 0,
            sequence: 0,
            epoch,
            finished: false,
        })
    }

    pub fn reset(&mut self, epoch: u64) {
        self.ring.fill(0.0);
        self.write = self.config.window_size - self.config.hop_size;
        self.pending = 0;
        self.samples = 0;
        self.sequence = 0;
        self.epoch = epoch;
        self.finished = false;
    }

    pub fn push(
        &mut self,
        samples: &[f32],
        mut emit: impl FnMut(SpectrumFrame),
    ) -> Result<(), TransformError> {
        if self.finished {
            return Err(TransformError::Finished);
        }
        for &sample in samples {
            self.samples += 1;
            if let Some(frame) = self.push_one(sample) {
                emit(frame);
            }
        }
        Ok(())
    }

    /// Flush the right boundary exactly once; no repeated trailing blocks.
    pub fn finish(&mut self, mut emit: impl FnMut(SpectrumFrame)) {
        if self.finished {
            return;
        }
        self.finished = true;
        if self.samples == 0 {
            return;
        }
        // Emit until the next block's finalized interval begins beyond real input.
        while self.next_start() < i64::try_from(self.samples).unwrap_or(i64::MAX) {
            if let Some(frame) = self.push_one(0.0) {
                emit(frame);
            }
        }
    }

    fn next_start(&self) -> i64 {
        let advance = self.sequence.saturating_mul(self.config.hop_size as u64);
        i64::try_from(advance).unwrap_or(i64::MAX)
            - i64::try_from(self.config.window_size - self.config.hop_size).unwrap_or(0)
    }

    fn push_one(&mut self, value: f32) -> Option<SpectrumFrame> {
        self.ring[self.write] = value;
        self.write = (self.write + 1) % self.ring.len();
        self.pending += 1;
        if self.pending != self.config.hop_size {
            return None;
        }
        self.pending = 0;
        for (i, slot) in self.input.iter_mut().enumerate() {
            *slot = self.ring[(self.write + i) % self.ring.len()] * self.window[i];
        }
        // Sizes are allocated by the same FFT plan and cannot mismatch.
        self.transform
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .expect("FFT plan buffers have matching sizes");
        let frame = SpectrumFrame {
            config: self.config,
            epoch: self.epoch,
            sequence: self.sequence,
            start_sample: self.next_start(),
            valid_samples: self.samples,
            bins: self.output.iter().map(|v| [v.re, v.im]).collect(),
        };
        self.sequence += 1;
        Some(frame)
    }
}

pub struct StreamingIfft {
    config: TransformConfig,
    transform: Arc<dyn ComplexToReal<f32>>,
    input: Vec<Complex32>,
    output: Vec<f32>,
    scratch: Vec<Complex32>,
    window: Vec<f32>,
    sum: Vec<f32>,
    weight: Vec<f32>,
    expected: Option<(u64, u64, i64)>,
}

impl StreamingIfft {
    pub fn new(config: TransformConfig) -> Result<Self, TransformError> {
        config.validate()?;
        let transform = RealFftPlanner::new().plan_fft_inverse(config.window_size);
        Ok(Self {
            input: transform.make_input_vec(),
            output: transform.make_output_vec(),
            scratch: transform.make_scratch_vec(),
            window: config.window(),
            sum: vec![0.0; config.window_size],
            weight: vec![0.0; config.window_size],
            config,
            transform,
            expected: None,
        })
    }

    pub fn reset(&mut self) {
        self.sum.fill(0.0);
        self.weight.fill(0.0);
        self.expected = None;
    }

    /// Emit finalized source-indexed samples. Never repeat or join missing blocks.
    #[allow(clippy::cast_precision_loss)] // Normalization by a validated, bounded FFT size.
    pub fn process(
        &mut self,
        frame: &SpectrumFrame,
        mut emit: impl FnMut(u64, f32),
    ) -> Result<(), TransformError> {
        if frame.config != self.config
            || self.expected.is_some_and(|expected| {
                expected != (frame.epoch, frame.sequence, frame.start_sample)
            })
            || (self.expected.is_none() && frame.sequence != 0)
        {
            return Err(TransformError::Discontinuity);
        }
        if frame.bins.len() != self.input.len()
            || frame.bins.iter().flatten().any(|v| !v.is_finite())
        {
            return Err(TransformError::InvalidSpectrum);
        }
        for (slot, &[re, im]) in self.input.iter_mut().zip(&frame.bins) {
            *slot = Complex32::new(re, im);
        }
        self.transform
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .map_err(|_| TransformError::InvalidSpectrum)?;
        for i in 0..self.config.window_size {
            self.sum[i] += self.output[i] * self.window[i] / self.config.window_size as f32;
            self.weight[i] = self.window[i].mul_add(self.window[i], self.weight[i]);
        }
        let hop = self.config.hop_size;
        for i in 0..hop {
            let index = frame.start_sample + i64::try_from(i).unwrap_or(0);
            if let Ok(index) = u64::try_from(index) {
                if index < frame.valid_samples && self.weight[i] > 1e-10 {
                    emit(index, self.sum[i] / self.weight[i]);
                }
            }
        }
        self.sum.rotate_left(hop);
        self.weight.rotate_left(hop);
        self.sum[self.config.window_size - hop..].fill(0.0);
        self.weight[self.config.window_size - hop..].fill(0.0);
        self.expected = Some((
            frame.epoch,
            frame.sequence + 1,
            frame.start_sample + i64::try_from(hop).unwrap_or(0),
        ));
        Ok(())
    }
}

impl SpectrumFrame {
    /// Project for display without modifying the complex data consumed by IFFT.
    #[allow(clippy::cast_precision_loss)] // Frequency axes and FFT normalization are f32.
    pub fn display(&self, mode: SpectrumOutput) -> SpectrumResult {
        let n = self.config.window_size;
        let window = self.config.window();
        let energy: f32 = window.iter().map(|w| w * w).sum();
        let gain: f32 = window.iter().sum();
        let mut values = Vec::with_capacity(self.bins.len());
        let mut frequencies = Vec::with_capacity(self.bins.len());
        for (k, &[re, im]) in self.bins.iter().enumerate() {
            let power = re.mul_add(re, im * im);
            // 单边谱: DC 与 Nyquist (n 为偶数时的 k=n/2) 不做 ×2 折叠
            let nyquist = n.is_multiple_of(2) && k == n / 2;
            let factor = if k == 0 || nyquist { 1.0 } else { 2.0 };
            let gain_sq = gain * gain;
            let normalized = factor * power / gain_sq;
            values.push(match mode {
                SpectrumOutput::Magnitude => factor * power.sqrt() / gain,
                SpectrumOutput::Power => normalized,
                SpectrumOutput::PSD => factor * power / (self.config.sample_rate * energy),
                SpectrumOutput::Decibel => 10.0 * normalized.max(1e-20).log10(),
            });
            frequencies.push(k as f32 * self.config.sample_rate / n as f32);
        }
        SpectrumResult {
            frequencies,
            values,
        }
    }
}
