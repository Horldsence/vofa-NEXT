//! 数字滤波器集成测试

use dsp_filter::{
    bandpass_biquad, bandstop_biquad, highpass_biquad, lowpass_biquad, DigitalFilter, FilterKind,
    FilterPreset,
};
use std::f32::consts::PI;

#[test]
fn test_fir_passthrough() {
    // b = [1.0] → FIR 通过 (y = x)
    let mut f = DigitalFilter::new(FilterKind::FIR { b: vec![1.0] });
    assert!((f.process(0.5) - 0.5).abs() < 1e-6);
    assert!((f.process(1.0) - 1.0).abs() < 1e-6);
}

#[test]
fn test_fir_delay() {
    // b = [0.0, 1.0] → 延迟一拍 (y[n] = x[n-1])
    let mut f = DigitalFilter::new(FilterKind::FIR { b: vec![0.0, 1.0] });
    assert!((f.process(1.0) - 0.0).abs() < 1e-6); // 第一次: x[-1]=0
    assert!((f.process(2.0) - 1.0).abs() < 1e-6); // 第二次: x[0]=1
    assert!((f.process(3.0) - 2.0).abs() < 1e-6); // 第三次: x[1]=2
}

#[test]
fn test_fir_moving_average() {
    // b = [0.5, 0.5] → 移动平均
    let mut f = DigitalFilter::new(FilterKind::FIR { b: vec![0.5, 0.5] });
    assert!((f.process(2.0) - 1.0).abs() < 1e-6); // (2+0)/2
    assert!((f.process(4.0) - 3.0).abs() < 1e-6); // (4+2)/2
    assert!((f.process(6.0) - 5.0).abs() < 1e-6); // (6+4)/2
}

#[test]
fn test_iir_passthrough() {
    // b = [1, 0, 0], a = [1, 0, 0] → y = x
    let mut f = DigitalFilter::new(FilterKind::IIR {
        b: [1.0, 0.0, 0.0],
        a: [1.0, 0.0, 0.0],
    });
    assert!((f.process(0.5) - 0.5).abs() < 1e-6);
}

#[test]
#[allow(clippy::cast_precision_loss)] // 测试信号循环计数数值小, 转 f32 无精度损失
fn test_lowpass_attenuates_high_freq() {
    // 采样率 1000 Hz, 截止 100 Hz
    // 输入 50 Hz (低频) 应通过, 400 Hz (高频) 应衰减
    let fs = 1000.0;
    let mut f_lp = DigitalFilter::from_preset(FilterPreset::Lowpass {
        cutoff: 100.0,
        sample_rate: fs,
    });
    let mut f_lp_high = DigitalFilter::from_preset(FilterPreset::Lowpass {
        cutoff: 100.0,
        sample_rate: fs,
    });

    // 测试 50 Hz (低频)
    let n = 200;
    let low_freq = 50.0;
    let mut max_lo = 0.0f32;
    for i in 0..n {
        let x = (2.0 * PI * low_freq * i as f32 / fs).sin();
        let y = f_lp.process(x);
        if i > 50 {
            // 稳态后测量幅值
            max_lo = max_lo.max(y.abs());
        }
    }

    // 测试 400 Hz (高频)
    let high_freq = 400.0;
    let mut max_hi = 0.0f32;
    for i in 0..n {
        let x = (2.0 * PI * high_freq * i as f32 / fs).sin();
        let y = f_lp_high.process(x);
        if i > 50 {
            max_hi = max_hi.max(y.abs());
        }
    }

    // 低频幅值应显著大于高频幅值 (衰减 > 50%)
    assert!(
        max_lo > max_hi * 2.0,
        "低频 {max_lo} 应明显大于高频 {max_hi} 的 2 倍"
    );
}

#[test]
#[allow(clippy::cast_precision_loss)] // 测试信号循环计数数值小, 转 f32 无精度损失
fn test_highpass_attenuates_low_freq() {
    let fs = 1000.0;
    let mut f_hp = DigitalFilter::from_preset(FilterPreset::Highpass {
        cutoff: 200.0,
        sample_rate: fs,
    });
    let mut f_hp_high = DigitalFilter::from_preset(FilterPreset::Highpass {
        cutoff: 200.0,
        sample_rate: fs,
    });

    // 低频 50 Hz (应衰减)
    let n = 200;
    let mut max_lo = 0.0f32;
    for i in 0..n {
        let x = (2.0 * PI * 50.0 * i as f32 / fs).sin();
        let y = f_hp.process(x);
        if i > 50 {
            max_lo = max_lo.max(y.abs());
        }
    }

    // 高频 400 Hz (应通过)
    let mut max_hi = 0.0f32;
    for i in 0..n {
        let x = (2.0 * PI * 400.0 * i as f32 / fs).sin();
        let y = f_hp_high.process(x);
        if i > 50 {
            max_hi = max_hi.max(y.abs());
        }
    }

    assert!(
        max_hi > max_lo * 2.0,
        "高频 {max_hi} 应明显大于低频 {max_lo} 的 2 倍"
    );
}

#[test]
fn test_iir_stability() {
    // biquad 系数 a0 应为正 (稳定的 biquad)
    let (b, a) = lowpass_biquad(100.0, 1000.0);
    assert!(a[0] > 0.0);
    assert!(b.iter().all(|v| v.is_finite()));
    assert!(a.iter().all(|v| v.is_finite()));
}

#[test]
fn test_reset() {
    let mut f = DigitalFilter::new(FilterKind::FIR { b: vec![0.5, 0.5] });
    f.process(1.0);
    f.process(2.0);
    f.reset();
    // 重置后输出应等同于首次处理
    let y = f.process(3.0);
    assert!((y - 1.5).abs() < 1e-6, "重置后 y = (3+0)/2 = 1.5, 实际 {y}");
}

#[test]
fn test_bandpass_basic() {
    let (b, a) = bandpass_biquad(100.0, 200.0, 1000.0);
    assert!(a[0] > 0.0);
    assert!(b.iter().all(|v| v.is_finite()));
    assert!(a.iter().all(|v| v.is_finite()));
}

#[test]
fn test_bandstop_basic() {
    let (b, a) = bandstop_biquad(100.0, 200.0, 1000.0);
    assert!(a[0] > 0.0);
    assert!(b.iter().all(|v| v.is_finite()));
    assert!(a.iter().all(|v| v.is_finite()));
}

#[test]
fn test_preset_to_filter() {
    let f = DigitalFilter::from_preset(FilterPreset::Lowpass {
        cutoff: 100.0,
        sample_rate: 1000.0,
    });
    // 应为 IIR 类型
    assert!(matches!(f.kind(), FilterKind::IIR { .. }));
}

#[test]
fn test_nyquist_guard_coefficients_finite() {
    // 越界 / 非法输入不应产生 NaN/Inf 或 a0 <= 0 (不稳定) 的系数
    let cases: [(f32, f32); 7] = [
        (1000.0, 1000.0), // cutoff == fs
        (2000.0, 1000.0), // cutoff == 2*fs
        (0.0, 1000.0),    // 零截止
        (-5.0, 1000.0),   // 负截止
        (f32::NAN, 1000.0),
        (100.0, 0.0),   // 零采样率
        (100.0, -10.0), // 负采样率
    ];
    for &(cutoff, fs) in &cases {
        for (name, (b, a)) in [
            ("lowpass", lowpass_biquad(cutoff, fs)),
            ("highpass", highpass_biquad(cutoff, fs)),
        ] {
            assert!(a[0] > 0.0, "{name} a0 应为正 (cutoff={cutoff}, fs={fs})");
            assert!(
                b.iter().chain(a.iter()).all(|v| v.is_finite()),
                "{name} 系数应为有限数 (cutoff={cutoff}, fs={fs})"
            );
        }
    }
}

#[test]
#[allow(clippy::cast_precision_loss)] // 斜坡信号循环计数数值小, 转 f32 无精度损失
fn test_nyquist_guard_stays_stable() {
    // 截止频率远超 Nyquist 时, 长期输入不应发散
    let mut f = DigitalFilter::from_preset(FilterPreset::Lowpass {
        cutoff: 5000.0,
        sample_rate: 1000.0,
    });
    for i in 0..10_000 {
        let x = (i as f32 * 0.1).sin();
        let y = f.process(x);
        assert!(y.is_finite(), "输出应为有限数 (i={i})");
        assert!(y.abs() < 1e6, "输出不应发散 (i={i}, y={y})");
    }
}

#[test]
fn test_band_low_high_swapped() {
    // low > high 时应交换, 系数有限且稳定
    for (name, (b, a)) in [
        ("bandpass", bandpass_biquad(200.0, 100.0, 1000.0)),
        ("bandstop", bandstop_biquad(200.0, 100.0, 1000.0)),
    ] {
        assert!(a[0] > 0.0, "{name} a0 应为正");
        assert!(
            b.iter().chain(a.iter()).all(|v| v.is_finite()),
            "{name} 系数应为有限数"
        );
    }
}
