// 测试信号生成中的数值转换 (采样率/相位/量化) 的精度损失与截断为预期行为
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use schema_types::{encode_by_blocks, ProtocolConfig, SchemaPreset, TestDataLink};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, watch, Notify};
use vofa_core::{TestDataConfig, TestSignal};

/// TestData 每条广播消息覆盖的目标采样时长。数据平面使用同一换算规则把
/// `broadcast::Lagged(消息数)` 还原成丢失帧数。
const MESSAGE_SAMPLE_SECONDS: f64 = 0.005;

#[doc(hidden)]
pub fn samples_per_message(sample_rate: f32) -> u64 {
    (f64::from(sample_rate.max(1.0)) * MESSAGE_SAMPLE_SECONDS)
        .ceil()
        .max(1.0) as u64
}

#[derive(Clone)]
pub struct TestDataRuntime {
    pub config: TestDataConfig,
    pub link: TestDataLink,
}

/// 启动测试数据生成器
///
/// `link` 决定生成数据的线缆格式 (protocol 为 legacy 配置, schema 为帧 schema):
/// - schema = Custom 且带 encode 块 → 按 schema 编码块生成
/// - 否则按 link.protocol 走 legacy 编码:
/// - JustFloat → 4 字节 LE float + 帧尾 [0x00,0x00,0x80,0x7f]
/// - FireWater → ASCII CSV `v1,v2,...,vn\n`
/// - RawData   → 递增字节流 (无解析)
/// - Slcan     → ASCII slcan 帧 `t<id><dlc><data>\r`
/// - CandleLight → 24 字节二进制 CAN 帧 (cmd=0x11 RX)
/// - LogicDecode → 字节流, 每字节代表 8 通道数字采样 (channel 0 输出方波)
///
/// 链路配置经 `watch` 通道传入, 返回其 `Sender` — 图/协议变化后可运行时热更新,
/// 无需重建生成任务 (生成循环每批数据读取最新值)。
#[allow(clippy::type_complexity)]
pub fn spawn(
    config: TestDataConfig,
    link: TestDataLink,
) -> vofa_core::Result<(
    mpsc::Sender<Vec<u8>>,
    broadcast::Sender<Vec<u8>>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    Arc<Notify>,
    watch::Sender<TestDataRuntime>,
)> {
    let (data_tx, _) = broadcast::channel(vofa_core::INGEST_CHANNEL_CAPACITY);
    let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(64);
    let (runtime_tx, mut runtime_rx) = watch::channel(TestDataRuntime { config, link });
    let cancel = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicBool::new(false));
    let notify = Arc::new(Notify::new());

    // 测试数据生成任务
    let data_tx_gen = data_tx.clone();
    let cancel_gen = cancel.clone();
    let running_gen = running.clone();
    let notify_gen = notify.clone();
    tokio::spawn(async move {
        let mut sample_idx: u64 = 0;
        // 采样时钟只由已生成样本数推进，批次调度抖动和热更新不会重置相位。
        let mut sample_time_sec = 0.0_f64;
        let mut previous_sample_dt = None;
        // 截止驱动节拍：保持绝对相位，短时调度迟到不改变名义采样率。
        // 每轮仍为固定大小消息；不按欠账分配大批内存。
        let mut next_deadline = tokio::time::Instant::now();
        loop {
            if cancel_gen.load(Ordering::Relaxed) {
                break;
            }
            if running_gen.load(Ordering::Relaxed) {
                let runtime = runtime_rx.borrow().clone();
                let channels = runtime.config.channels.max(1);
                let sample_rate = runtime.config.sample_rate.max(1.0);
                let signal = runtime.config.signal;
                // 每条消息覆盖约 5ms，避开毫秒级定时器对亚毫秒节拍的限速。
                // 每轮重算使
                // sample_rate/channels/signal 与协议一样可在连接期间热更新。
                let samples_per_msg = samples_per_message(sample_rate);
                let msg_interval =
                    Duration::from_secs_f64(samples_per_msg as f64 / f64::from(sample_rate));
                let sample_dt = 1.0_f64 / f64::from(sample_rate);
                if let Some(previous_dt) = previous_sample_dt.replace(sample_dt) {
                    if (previous_dt - sample_dt).abs() > f64::EPSILON {
                        // sample_time_sec 指向下一个样本；热更新后把该间隔替换为新间隔，
                        // 但不改变已经生成的相位历史。
                        sample_time_sec += sample_dt - previous_dt;
                    }
                }
                tokio::select! {
                    () = tokio::time::sleep_until(next_deadline) => {
                        // 每轮恒定批量；保留截止时间使迟到批次逐轮补齐。
                        next_deadline += msg_interval;
                        let mut data = Vec::new();
                        for _ in 0..samples_per_msg {
                            data.extend_from_slice(&generate_link_bytes(
                                channels,
                                signal,
                                sample_time_sec,
                                &runtime.link,
                                sample_idx,
                            ));
                            sample_idx += 1;
                            sample_time_sec += sample_dt;
                        }

                        let _ = data_tx_gen.send(data);
                    }
                    changed = runtime_rx.changed() => {
                        if changed.is_err() { break; }
                    }
                    () = notify_gen.notified() => {}
                    data = write_rx.recv() => {
                        // 写入由 TransportHandle::send 统一回环到 data_tx 广播,
                        // 这里只需排空写入通道 (通道满会导致 send 报错)
                        if data.is_none() { break; }
                    }
                    () = tokio::time::sleep(Duration::from_millis(100)) => {
                        if cancel_gen.load(Ordering::Relaxed) { break; }
                    }
                }
            } else {
                tokio::select! {
                    () = notify_gen.notified() => {}
                    data = write_rx.recv() => {
                        if data.is_none() { break; }
                    }
                    () = tokio::time::sleep(Duration::from_millis(100)) => {
                        if cancel_gen.load(Ordering::Relaxed) { break; }
                    }
                }
                // 暂停时间不属于采集时间，恢复时不补发整个暂停期间的样本。
                next_deadline = tokio::time::Instant::now();
            }
        }
        log::debug!("测试数据生成器退出");
    });

    Ok((write_tx, data_tx, cancel, running, notify, runtime_tx))
}

/// 按链路配置生成线缆格式的字节流
///
/// 编码路径选择:
/// - schema = Some(Custom) 且 encode = Some → 按 schema 编码块编码 values
///   (values 与 decode 块派生的端口列表对齐)
/// - schema = Some(预设) 或 None → 现有 generate_bytes legacy 路径
fn generate_link_bytes(
    channels: usize,
    signal: TestSignal,
    t: f64,
    link: &TestDataLink,
    sample_idx: u64,
) -> Vec<u8> {
    if let Some(schema) = &link.schema {
        if schema.preset == SchemaPreset::Custom {
            if let Some(encode) = &schema.encode {
                let frame = generate_frame(channels, signal, t);
                return encode_by_blocks(encode, &schema.port_names(), &frame);
            }
        }
    }
    generate_bytes(channels, signal, t, &link.protocol, sample_idx)
}

/// 按协议类型生成线缆格式的字节流 (legacy 路径)
///
/// `pub` 用于 `tests/` 集成测试验证字节格式; 协议层不应直接调用。
#[doc(hidden)]
pub fn generate_bytes(
    channels: usize,
    signal: TestSignal,
    t: f64,
    protocol: &ProtocolConfig,
    sample_idx: u64,
) -> Vec<u8> {
    let frame = generate_frame(channels, signal, t);
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
            let s: Vec<String> = frame.iter().map(|v| format!("{v:.6}")).collect();
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
            let mut s = format!("t{id:03X}{dlc:X}");
            for &b in &data_bytes {
                let _ = write!(s, "{b:02X}");
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
            let square_bit = u8::from(sample_idx.is_multiple_of(2));
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
        ProtocolConfig::Diagnostic { .. } => {
            // 诊断模式走独立的 DiagnosticEngine + BridgeCanBackend 管线,
            // test_data 不适用,返回空字节(由上层判断是否发送)。
            Vec::new()
        }
    }
}

/// 生成一帧通道浮点值 (与原实现保持一致)
/// t 为生成器启动以来的真实流逝时间 (秒), 作为所有信号的时间基准
///
/// `pub` 用于 `tests/` 集成测试验证信号形状; 协议层不应直接调用。
#[doc(hidden)]
pub fn generate_frame(channels: usize, signal: TestSignal, t: f64) -> Vec<f32> {
    (0..channels)
        .map(|i| {
            let channel = i as f64;
            let freq = 1.0 + channel;
            let p = t * freq * 2.0 * std::f64::consts::PI;
            match signal {
                TestSignal::Sine => {
                    (p.sin() * channel.mul_add(0.5, 1.0)).mul_add(50.0, 128.0) as f32
                }
                TestSignal::Square => {
                    if p.sin() > 0.0 {
                        channel.mul_add(10.0, 200.0) as f32
                    } else {
                        channel.mul_add(10.0, 50.0) as f32
                    }
                }
                TestSignal::Triangle => {
                    let normalized = (p / std::f64::consts::PI) % 2.0;
                    let tri = if normalized < 1.0 {
                        normalized
                    } else {
                        2.0 - normalized
                    };
                    channel.mul_add(20.0, tri * 100.0) as f32
                }
                TestSignal::Sawtooth => {
                    let normalized = t * freq % 1.0;
                    channel.mul_add(10.0, normalized * 200.0) as f32
                }
                TestSignal::Random => {
                    // 简单的伪随机: 基于时间的 hash
                    let seed = t.mul_add(1000.0, channel);
                    let r = (seed.sin() * 43_758.547).fract();
                    (r * 255.0) as f32
                }
                TestSignal::Dc => {
                    // 直流: 每通道一个固定值
                    channel.mul_add(20.0, 128.0) as f32
                }
                TestSignal::Chirp => {
                    // 扫频: 频率随时间线性增加
                    let f = t.mul_add(2.0, 0.5);
                    channel.mul_add(
                        10.0,
                        (t * f * freq * 2.0 * std::f64::consts::PI)
                            .sin()
                            .mul_add(80.0, 128.0),
                    ) as f32
                }
                TestSignal::Steps => {
                    // 阶梯: 每 10 个采样点上升一级
                    let step = (t * freq * 5.0) as i32;
                    channel.mul_add(10.0, f64::from(step).rem_euclid(8.0).mul_add(30.0, 20.0))
                        as f32
                }
                TestSignal::Noise => {
                    // 高斯噪声近似 (Box-Muller 简化版)
                    let seed1 = channel.mul_add(7.0, t * 1000.0);
                    let seed2 = channel.mul_add(13.0, t * 999.0);
                    let u1 = (seed1.sin() * 43_758.547).fract().max(0.0001);
                    let u2 = (seed2.sin() * 12_345.679).fract();
                    let n = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                    // 标准正态 → 缩放到 0..255
                    (n * 40.0 + 128.0).clamp(0.0, 255.0) as f32
                }
                TestSignal::MultiTone => {
                    // 多频叠加: 基频 + 3次谐波 + 5次谐波
                    let base = p.sin();
                    let h3 = (p * 3.0).sin() * 0.33;
                    let h5 = (p * 5.0).sin() * 0.2;
                    channel.mul_add(10.0, (base + h3 + h5).mul_add(60.0, 128.0)) as f32
                }
            }
        })
        .collect()
}
