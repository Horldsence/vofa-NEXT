use serial_core::{TestDataConfig, TestSignal};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};

/// 启动测试数据生成器
pub async fn spawn(
    config: TestDataConfig,
) -> serial_core::Result<(
    mpsc::Sender<Vec<u8>>,
    broadcast::Sender<Vec<u8>>,
    Arc<AtomicBool>,
)> {
    let (data_tx, _) = broadcast::channel(256);
    let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(64);
    let cancel = Arc::new(AtomicBool::new(false));

    let channels = config.channels.max(1);
    let sample_rate = config.sample_rate.max(1.0);
    let signal = config.signal;
    let interval_us = (1_000_000.0 / sample_rate) as u64;

    // 测试数据生成任务
    let data_tx_gen = data_tx.clone();
    let cancel_gen = cancel.clone();
    tokio::spawn(async move {
        let start = Instant::now();
        let mut sample_idx: u64 = 0;
        let interval = Duration::from_micros(interval_us);
        let mut tick = tokio::time::interval(interval);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    let t = start.elapsed().as_secs_f32();
                    let phase = sample_idx as f32 / sample_rate;
                    let frame = generate_frame(channels, signal, t, phase);

                    // 生成 JustFloat 格式数据
                    let mut data = Vec::with_capacity(channels * 4 + 4);
                    for v in &frame {
                        data.extend_from_slice(&v.to_le_bytes());
                    }
                    data.extend_from_slice(&[0x00, 0x00, 0x80, 0x7f]); // JustFloat tail

                    let _ = data_tx_gen.send(data);
                    sample_idx += 1;
                }
                data = write_rx.recv() => {
                    // 测试数据模式忽略写入
                    if data.is_none() { break; }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if cancel_gen.load(Ordering::Relaxed) { break; }
                }
            }
        }
        tracing::info!("测试数据生成器退出");
    });

    Ok((write_tx, data_tx, cancel))
}

fn generate_frame(channels: usize, signal: TestSignal, t: f32, phase: f32) -> Vec<f32> {
    (0..channels)
        .map(|i| {
            let freq = 1.0 + i as f32;
            let p = phase * freq * 2.0 * std::f32::consts::PI;
            match signal {
                TestSignal::Sine => p.sin() * (1.0 + i as f32 * 0.5) * 50.0 + 128.0,
                TestSignal::Square => {
                    if p.sin() > 0.0 {
                        200.0 + i as f32 * 10.0
                    } else {
                        50.0 + i as f32 * 10.0
                    }
                }
                TestSignal::Triangle => {
                    let normalized = (p / std::f32::consts::PI) % 2.0;
                    let tri = if normalized < 1.0 {
                        normalized
                    } else {
                        2.0 - normalized
                    };
                    tri * 100.0 + i as f32 * 20.0
                }
                TestSignal::Sawtooth => {
                    let normalized = phase * freq % 1.0;
                    normalized * 200.0 + i as f32 * 10.0
                }
                TestSignal::Random => {
                    // 简单的伪随机: 基于时间的 hash
                    let seed = t * 1000.0 + i as f32;
                    let r = (seed.sin() * 43758.5453).fract();
                    r * 255.0
                }
                TestSignal::Dc => {
                    // 直流: 每通道一个固定值
                    128.0 + i as f32 * 20.0
                }
                TestSignal::Chirp => {
                    // 扫频: 频率随时间线性增加
                    let f = 0.5 + t * 2.0;
                    (phase * f * freq * 2.0 * std::f32::consts::PI).sin()
                        * 80.0
                        + 128.0
                        + i as f32 * 10.0
                }
                TestSignal::Steps => {
                    // 阶梯: 每 10 个采样点上升一级
                    let step = ((phase * freq * 5.0) as i32) as f32;
                    (step.rem_euclid(8.0) * 30.0) + 20.0 + i as f32 * 10.0
                }
                TestSignal::Noise => {
                    // 高斯噪声近似 (Box-Muller 简化版)
                    let seed1 = t * 1000.0 + i as f32 * 7.0;
                    let seed2 = t * 999.0 + i as f32 * 13.0;
                    let u1 = (seed1.sin() * 43758.5453).fract().max(0.0001);
                    let u2 = (seed2.sin() * 12345.6789).fract();
                    let n = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos();
                    // 标准正态 → 缩放到 0..255
                    (n * 40.0 + 128.0).clamp(0.0, 255.0)
                }
                TestSignal::MultiTone => {
                    // 多频叠加: 基频 + 3次谐波 + 5次谐波
                    let base = p.sin();
                    let h3 = (p * 3.0).sin() * 0.33;
                    let h5 = (p * 5.0).sin() * 0.2;
                    (base + h3 + h5) * 60.0 + 128.0 + i as f32 * 10.0
                }
            }
        })
        .collect()
}
