use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, Notify};
use vofa_next_core::{ProtocolConfig, TestDataConfig, TestSignal};

/// 启动测试数据生成器
///
/// `protocol` 决定生成数据的线缆格式:
/// - JustFloat → 4 字节 LE float + 帧尾 [0x00,0x00,0x80,0x7f]
/// - FireWater → ASCII CSV `v1,v2,...,vn\n`
/// - RawData   → 递增字节流 (无解析)
/// - Slcan     → ASCII slcan 帧 `t<id><dlc><data>\r`
/// - CandleLight → 24 字节二进制 CAN 帧 (cmd=0x11 RX)
/// - LogicDecode → 字节流, 每字节代表 8 通道数字采样 (channel 0 输出方波)
pub async fn spawn(
    config: TestDataConfig,
    protocol: ProtocolConfig,
) -> vofa_next_core::Result<(
    mpsc::Sender<Vec<u8>>,
    broadcast::Sender<Vec<u8>>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    Arc<Notify>,
)> {
    let (data_tx, _) = broadcast::channel(2048);
    let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(64);
    let cancel = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicBool::new(false));
    let notify = Arc::new(Notify::new());

    let channels = config.channels.max(1);
    let sample_rate = config.sample_rate.max(1.0);
    let signal = config.signal;
    let interval_us = (1_000_000.0 / sample_rate) as u64;

    // 测试数据生成任务
    let data_tx_gen = data_tx.clone();
    let cancel_gen = cancel.clone();
    let running_gen = running.clone();
    let notify_gen = notify.clone();
    tokio::spawn(async move {
        let start = Instant::now();
        let mut sample_idx: u64 = 0;
        let interval = Duration::from_micros(interval_us);
        let mut tick = tokio::time::interval(interval);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            if running_gen.load(Ordering::Relaxed) {
                tokio::select! {
                    _ = tick.tick() => {
                        let t = start.elapsed().as_secs_f32();
                        let phase = sample_idx as f32 / sample_rate;
                        let data = generate_bytes(channels, signal, t, phase, &protocol, sample_idx);

                        let _ = data_tx_gen.send(data);
                        sample_idx += 1;
                    }
                    _ = notify_gen.notified() => {}
                    data = write_rx.recv() => {
                        // 测试数据模式忽略写入
                        if data.is_none() { break; }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        if cancel_gen.load(Ordering::Relaxed) { break; }
                    }
                }
            } else {
                tokio::select! {
                    _ = notify_gen.notified() => {}
                    data = write_rx.recv() => {
                        if data.is_none() { break; }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        if cancel_gen.load(Ordering::Relaxed) { break; }
                    }
                }
            }
        }
        log::info!("测试数据生成器退出");
    });

    Ok((write_tx, data_tx, cancel, running, notify))
}

/// 按协议类型生成线缆格式的字节流
fn generate_bytes(
    channels: usize,
    signal: TestSignal,
    t: f32,
    phase: f32,
    protocol: &ProtocolConfig,
    sample_idx: u64,
) -> Vec<u8> {
    let frame = generate_frame(channels, signal, t, phase);
    match protocol {
        ProtocolConfig::JustFloat { .. } => {
            // 4 字节 LE float + 帧尾
            let mut data = Vec::with_capacity(channels * 4 + 4);
            for v in &frame {
                data.extend_from_slice(&v.to_le_bytes());
            }
            data.extend_from_slice(&[0x00, 0x00, 0x80, 0x7f]); // JustFloat tail
            data
        }
        ProtocolConfig::FireWater { .. } => {
            // CSV 文本: v1,v2,...,vn\n
            let s: Vec<String> = frame.iter().map(|v| format!("{:.6}", v)).collect();
            let mut data = s.join(",").into_bytes();
            data.push(b'\n');
            data
        }
        ProtocolConfig::RawData => {
            // 原始字节: 每通道值取低 8 位, 直接拼接
            let mut data = Vec::with_capacity(channels);
            for v in &frame {
                data.push((*v).clamp(0.0, 255.0) as u8);
            }
            // 附加 4 字节计数器以便观察
            data.extend_from_slice(&(sample_idx as u32).to_le_bytes());
            data
        }
        ProtocolConfig::Slcan => {
            // ASCII slcan 数据帧: t<id3><dlc><data>\r
            // ID 与 data 由 frame 值推导, 每帧一个 CAN 报文
            let id = (sample_idx % 0x800) as u32;
            let dlc = 8;
            let mut data_bytes = [0u8; 8];
            for i in 0..8 {
                let v = if i < frame.len() { frame[i] } else { 0.0 };
                data_bytes[i] = v.clamp(0.0, 255.0) as u8;
            }
            let mut s = format!("t{:03X}{:X}", id, dlc);
            for &b in &data_bytes {
                s.push_str(&format!("{:02X}", b));
            }
            s.push('\r');
            s.into_bytes()
        }
        ProtocolConfig::CandleLight => {
            // 24 字节二进制 CAN RX 帧 (cmd=0x11)
            let mut pkt = [0u8; 24];
            pkt[0] = 0x11; // CAND_CMD_RX
            let can_id = (sample_idx % 0x800) as u32;
            pkt[8..12].copy_from_slice(&can_id.to_le_bytes());
            pkt[12] = 8; // dlc
            for i in 0..8 {
                let v = if i < frame.len() { frame[i] } else { 0.0 };
                pkt[16 + i] = v.clamp(0.0, 255.0) as u8;
            }
            pkt.to_vec()
        }
        ProtocolConfig::LogicDecode { .. } => {
            // 每字节 = 一个 8 通道数字采样 (bit i = 通道 i 电平)
            // 在通道 0 产生方波, 其余通道跟随 frame 值阈值化
            let mut data = Vec::with_capacity(channels.max(1));
            let square_bit = if sample_idx.is_multiple_of(2) { 0x01 } else { 0x00 };
            let mut bits: u8 = square_bit;
            for i in 1..8 {
                let v = if i < frame.len() { frame[i] } else { 0.0 };
                if v > 128.0 {
                    bits |= 1 << i;
                }
            }
            data.push(bits);
            // 每个采样间隔产生 8 个等距采样, 让解码器有数据可解
            for _ in 0..7 {
                data.push(bits);
            }
            data
        }
    }
}

/// 生成一帧通道浮点值 (与原实现保持一致)
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
                    let r = (seed.sin() * 43_758.547).fract();
                    r * 255.0
                }
                TestSignal::Dc => {
                    // 直流: 每通道一个固定值
                    128.0 + i as f32 * 20.0
                }
                TestSignal::Chirp => {
                    // 扫频: 频率随时间线性增加
                    let f = 0.5 + t * 2.0;
                    (phase * f * freq * 2.0 * std::f32::consts::PI).sin() * 80.0
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
                    let u1 = (seed1.sin() * 43_758.547).fract().max(0.0001);
                    let u2 = (seed2.sin() * 12_345.679).fract();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_justfloat_format() {
        let protocol = ProtocolConfig::JustFloat { channels: Some(2) };
        let data = generate_bytes(2, TestSignal::Sine, 0.0, 0.0, &protocol, 0);
        // 2 channels * 4 bytes + 4 byte tail
        assert_eq!(data.len(), 12);
        // 帧尾
        assert_eq!(&data[8..12], &[0x00, 0x00, 0x80, 0x7f]);
    }

    #[test]
    fn test_firewater_format() {
        let protocol = ProtocolConfig::FireWater { channels: Some(2) };
        let data = generate_bytes(2, TestSignal::Sine, 0.0, 0.0, &protocol, 0);
        let s = String::from_utf8(data.clone()).unwrap();
        assert!(s.ends_with('\n'));
        assert_eq!(s.matches(',').count(), 1);
    }

    #[test]
    fn test_slcan_format() {
        let protocol = ProtocolConfig::Slcan;
        let data = generate_bytes(8, TestSignal::Square, 0.0, 0.0, &protocol, 0);
        let s = String::from_utf8(data).unwrap();
        assert!(s.starts_with('t'));
        assert!(s.ends_with('\r'));
        // t + 3 (id) + 1 (dlc) + 16 (8 bytes hex) + 1 (\r) = 22
        assert_eq!(s.len(), 22);
    }

    #[test]
    fn test_candle_format() {
        let protocol = ProtocolConfig::CandleLight;
        let data = generate_bytes(8, TestSignal::Square, 0.0, 0.0, &protocol, 0);
        assert_eq!(data.len(), 24);
        assert_eq!(data[0], 0x11); // RX cmd
        assert_eq!(data[12], 8); // dlc
    }

    #[test]
    fn test_rawdata_format() {
        let protocol = ProtocolConfig::RawData;
        let data = generate_bytes(4, TestSignal::Dc, 0.0, 0.0, &protocol, 42);
        // 4 channel bytes + 4 byte counter
        assert_eq!(data.len(), 8);
        assert_eq!(&data[4..8], &42u32.to_le_bytes());
    }

    #[test]
    fn test_logic_decode_format() {
        let protocol = ProtocolConfig::LogicDecode {
            decoder: vofa_next_core::LogicDecoderConfig::Uart {
                baud_rate: 115200,
                data_bits: 8,
                parity: vofa_next_core::Parity::None,
                stop_bits: vofa_next_core::StopBits::One,
                channel: 0,
            },
        };
        let data = generate_bytes(8, TestSignal::Square, 0.0, 0.0, &protocol, 0);
        // 8 samples per tick
        assert_eq!(data.len(), 8);
        // 通道 0 应有方波翻转
        assert_ne!(data[0] & 0x01, 0);
    }
}
