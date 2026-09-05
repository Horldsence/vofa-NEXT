//! FFT 加速自相关 — 周期检测的数值底座 (有偏归一化 ACF)
//!
//! 利用卷积定理: ACF = IFFT(|FFT(x)|²)。零填充至 ≥2n 的 2 的幂避免环绕混叠,
//! 输出为标准有偏归一化自相关 (r[lag] = Σx[i]x[i+lag] / Σx²): 随 lag 自然衰减,
//! 利于周期检测的「首个显著峰」搜索 (不会在大 lag 处被噪声抬升误导)。

use realfft::RealFftPlanner;

/// 有偏归一化自相关 `r[0..=max_lag]` (r[0] = 1.0)。
///
/// 内部先去均值 — 直流分量只落在 DC bin, 去均值后的 ACF 才是有效的周期
/// 结构度量。输入须为有限值 (NaN 直接拒绝); 样本数 < 4 或去均值后能量为零
/// (直流/全零) 时返回 None。
pub fn normalized_autocorrelation(samples: &[f32], max_lag: usize) -> Option<Vec<f32>> {
    let n = samples.len();
    if n < 4 || samples.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let max_lag = max_lag.min(n - 1).max(1);
    #[allow(clippy::cast_precision_loss)] // 计数转均值, 点数受调用方窗口约束
    let mean = samples.iter().map(|&v| f64::from(v)).sum::<f64>() / n as f64;
    #[allow(clippy::cast_possible_truncation)] // 去均值后幅值远超 f32 下溢区间
    let centered: Vec<f32> = samples
        .iter()
        .map(|&v| (f64::from(v) - mean) as f32)
        .collect();
    let fft_len = (n * 2).next_power_of_two().max(4);

    let mut planner = RealFftPlanner::<f32>::new();
    let forward = planner.plan_fft_forward(fft_len);
    let inverse = planner.plan_fft_inverse(fft_len);

    let mut input = forward.make_input_vec();
    input[..n].copy_from_slice(&centered);
    input[n..].fill(0.0);
    let mut spectrum = forward.make_output_vec();
    let mut scratch = forward.make_scratch_vec();
    forward
        .process_with_scratch(&mut input, &mut spectrum, &mut scratch)
        .ok()?;

    // 功率谱 |X(k)|² = X·conj(X) — 逆变换即自相关
    for bin in &mut spectrum {
        *bin *= bin.conj();
    }

    let mut acf = inverse.make_output_vec();
    let mut inverse_scratch = inverse.make_scratch_vec();
    inverse
        .process_with_scratch(&mut spectrum, &mut acf, &mut inverse_scratch)
        .ok()?;

    // realfft 逆变换不归一化: 除以 fft_len
    #[allow(clippy::cast_precision_loss)] // fft_len 为 2 的幂, 精度损失可忽略
    let scale = 1.0 / fft_len as f64;
    let r0 = f64::from(acf[0]) * scale;
    if r0 <= f64::EPSILON {
        return None;
    }
    let mut out = Vec::with_capacity(max_lag + 1);
    out.extend(acf[..=max_lag].iter().map(|&value| {
        #[allow(clippy::cast_possible_truncation)] // ACF 值域 [-1,1], f32 精度足够峰搜索
        let normalized = (f64::from(value) * scale / r0) as f32;
        normalized
    }));
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_autocorrelation_peaks_at_period() {
        // 1kHz 正弦 @ 100kHz 采样: 周期 100 样本
        let n = 4096;
        let samples: Vec<f32> = (0..n)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f32 / 100_000.0;
                (std::f32::consts::TAU * 1_000.0 * t).sin()
            })
            .collect();
        let acf = normalized_autocorrelation(&samples, n / 2).expect("正弦有能量");
        assert!((acf[0] - 1.0).abs() < 1e-5);
        // 首个显著峰应落在周期 100 样本附近
        let peak = (2..acf.len() - 1)
            .find(|&k| acf[k] >= 0.35 && acf[k] > acf[k - 1] && acf[k] >= acf[k + 1])
            .expect("周期信号必有 ACF 峰");
        #[allow(clippy::cast_precision_loss)]
        let peak_f = peak as f64;
        assert!((peak_f - 100.0).abs() <= 2.0, "peak={peak}");
    }

    #[test]
    fn dc_signal_returns_none() {
        let samples = vec![2.5_f32; 256];
        assert!(normalized_autocorrelation(&samples, 32).is_none());
    }

    #[test]
    fn nan_samples_return_none() {
        let mut samples = vec![1.0_f32; 64];
        samples[10] = f32::NAN;
        assert!(normalized_autocorrelation(&samples, 8).is_none());
    }
}
