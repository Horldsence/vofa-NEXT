#![allow(clippy::cast_precision_loss)]
// 测试信号与断言阈值按数学形式书写 — mul_add 改写无收益
#![allow(clippy::suboptimal_flops)]

use dsp_fft::{StreamingFft, StreamingIfft, TransformConfig, TransformError, WindowType};

const fn config(window_type: WindowType, hop_size: usize) -> TransformConfig {
    TransformConfig {
        window_size: 128,
        hop_size,
        window_type,
        sample_rate: 48_000.0,
    }
}

#[test]
fn phase_and_boundaries_survive_arbitrary_input_chunks() {
    for window in [
        WindowType::Rect,
        WindowType::Hann,
        WindowType::Hamming,
        WindowType::Blackman,
    ] {
        for hop in [31, 32, 64] {
            let cfg = config(window, hop);
            let mut fft = StreamingFft::new(cfg, 42).unwrap();
            let mut ifft = StreamingIfft::new(cfg).unwrap();
            let samples: Vec<_> = (0..1037)
                .map(|i| {
                    let t = i as f32;
                    0.4 * (t * 0.037 + 0.71).sin()
                        + 0.3 * (t * 0.49 - 1.2).cos()
                        + if i == 0 || i == 1036 || i == 157 {
                            0.2
                        } else {
                            0.0
                        }
                })
                .collect();
            let mut output = Vec::new();
            let mut accept = |frame| {
                ifft.process(&frame, |index, value| {
                    assert_eq!(index, output.len() as u64);
                    output.push(value);
                })
                .unwrap();
            };
            for chunk in samples.chunks(17) {
                fft.push(chunk, &mut accept).unwrap();
            }
            fft.finish(&mut accept);
            fft.finish(|_| panic!("finish must be idempotent"));
            assert_eq!(output.len(), samples.len(), "{window:?}, hop={hop}");
            for (a, b) in samples.iter().zip(&output) {
                assert!(
                    (a - b).abs() <= 1e-5 + 1e-4 * a.abs(),
                    "{window:?}, hop={hop}: {a} != {b}"
                );
            }
        }
    }
}

#[test]
fn rectangular_nonoverlapping_short_stream_and_empty_stream() {
    for count in [0, 1, 127, 128, 129, 1024] {
        let cfg = config(WindowType::Rect, 128);
        let mut fft = StreamingFft::new(cfg, 0).unwrap();
        let mut ifft = StreamingIfft::new(cfg).unwrap();
        let mut output = Vec::new();
        let mut accept = |frame| ifft.process(&frame, |_, v| output.push(v)).unwrap();
        fft.push(&vec![0.75; count], &mut accept).unwrap();
        fft.finish(&mut accept);
        assert_eq!(output.len(), count);
        assert!(output.iter().all(|v| (v - 0.75).abs() < 1e-5));
    }
}

#[test]
fn rejects_missing_duplicate_and_foreign_epoch_blocks() {
    let cfg = config(WindowType::Hann, 64);
    let mut fft = StreamingFft::new(cfg, 5).unwrap();
    let mut frames = Vec::new();
    fft.push(&[0.5; 256], |frame| frames.push(frame)).unwrap();
    let mut ifft = StreamingIfft::new(cfg).unwrap();
    ifft.process(&frames[0], |_, _| {}).unwrap();
    assert_eq!(
        ifft.process(&frames[0], |_, _| {}),
        Err(TransformError::Discontinuity)
    );
    assert_eq!(
        ifft.process(&frames[2], |_, _| {}),
        Err(TransformError::Discontinuity)
    );
    let mut foreign = frames[1].clone();
    foreign.epoch += 1;
    assert_eq!(
        ifft.process(&foreign, |_, _| {}),
        Err(TransformError::Discontinuity)
    );
    ifft.process(&frames[1], |_, _| {}).unwrap();
    ifft.reset();
    fft.reset(6);
    fft.push(&[0.5; 64], |frame| ifft.process(&frame, |_, _| {}).unwrap())
        .unwrap();
}

#[test]
fn validates_coverage_sizes_and_sampling_rate() {
    assert_eq!(
        config(WindowType::Hann, 128).validate(),
        Err(TransformError::UncoveredWindow)
    );
    for hop in [0, 129] {
        assert_eq!(
            config(WindowType::Rect, hop).validate(),
            Err(TransformError::InvalidConfig)
        );
    }
    let mut cfg = config(WindowType::Hann, 64);
    cfg.sample_rate = f32::NAN;
    assert_eq!(cfg.validate(), Err(TransformError::InvalidConfig));
}
