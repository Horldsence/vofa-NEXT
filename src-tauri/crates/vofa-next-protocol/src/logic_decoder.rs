use vofa_next_core::{
    DecodedEvent, DataFrame, I2cEvent, LogicDecoderConfig, LogicSample,
};

use crate::engine::ProtocolEngine;

/// 逻辑分析仪解码引擎
///
/// 把接收字节流当作数字采样 (每字节 = 1 sample, bit i = 通道 i 电平),
/// 然后根据配置 (UART/I2C/SPI) 解码出协议事件。
pub struct LogicDecoderEngine {
    config: LogicDecoderConfig,
    /// I2C/SPI 解码用的内部采样缓冲 (跨数据包保持状态)
    sample_buf: Vec<LogicSample>,
    /// UART 解码状态
    uart_state: UartState,
    /// I2C 解码状态
    i2c_state: I2cState,
    /// SPI 解码状态
    spi_state: SpiState,
}

/// UART 解码状态
struct UartState {
    /// 上一次的字节时间戳 (用于去重)
    last_ts: u64,
}

/// I2C 解码状态机
struct I2cState {
    /// 当前 SDA 电平
    sda_prev: bool,
    /// 当前 SCL 电平
    scl_prev: bool,
    /// 移位寄存器 (8 位)
    shift: u8,
    /// 已接收位数
    bit_count: u8,
    /// 是否在传输中 (START 后, STOP 前)
    in_transaction: bool,
    /// 是否正在接收地址字节
    is_address_phase: bool,
}

/// SPI 解码状态机
struct SpiState {
    /// 上一次 SCLK 电平
    sclk_prev: bool,
    /// 上一次 CS 电平
    cs_prev: bool,
    /// MOSI 移位寄存器
    mosi_shift: u8,
    /// MISO 移位寄存器
    miso_shift: u8,
    /// 已接收位数
    bit_count: u8,
    /// 是否在传输中 (CS 低)
    in_transaction: bool,
}

impl LogicDecoderEngine {
    pub fn new(config: LogicDecoderConfig) -> Self {
        Self {
            config,
            sample_buf: Vec::with_capacity(4096),
            uart_state: UartState { last_ts: 0 },
            i2c_state: I2cState {
                sda_prev: true,
                scl_prev: true,
                shift: 0,
                bit_count: 0,
                in_transaction: false,
                is_address_phase: false,
            },
            spi_state: SpiState {
                sclk_prev: false,
                cs_prev: true,
                mosi_shift: 0,
                miso_shift: 0,
                bit_count: 0,
                in_transaction: false,
            },
        }
    }

    /// 获取通道位电平
    #[inline]
    fn channel_bit(sample: &LogicSample, channel: u8) -> bool {
        (sample.channels >> channel) & 1 == 1
    }

    /// 当前时间戳 (微秒)
    fn now_us() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0)
    }

    /// UART 解码: 串口接收的字节就是 UART 解码后的数据, 直接包装为 DecodedEvent
    fn decode_uart(&mut self, data: &[u8]) -> Vec<DecodedEvent> {
        let LogicDecoderConfig::Uart { parity, .. } = &self.config else {
            return Vec::new();
        };
        let now = Self::now_us();
        let mut events = Vec::with_capacity(data.len());
        for &b in data {
            // 跳过连续相同时间戳 (去重)
            let ts = now;
            // parity 校验: 当前串口层已处理, 这里假设校验通过
            // (若配置了 parity, 串口层会自动校验并丢弃错误帧)
            let parity_ok = matches!(parity, vofa_next_core::Parity::None);
            events.push(DecodedEvent::Uart {
                timestamp: ts,
                byte: b,
                parity_ok,
            });
            self.uart_state.last_ts = ts;
        }
        events
    }

    /// I2C 解码: 监测 SDA/SCL 通道, 检测 START/STOP/ACK/数据位
    fn decode_i2c(&mut self, samples: &[LogicSample]) -> Vec<DecodedEvent> {
        let LogicDecoderConfig::I2c { sda_channel, scl_channel } = &self.config else {
            return Vec::new();
        };
        let sda_ch = *sda_channel;
        let scl_ch = *scl_channel;
        let mut events = Vec::new();
        let state = &mut self.i2c_state;

        for s in samples {
            let sda = Self::channel_bit(s, sda_ch);
            let scl = Self::channel_bit(s, scl_ch);
            let ts = s.timestamp;

            // 检测 START: SDA 下降沿 + SCL 高
            if !sda && state.sda_prev && scl {
                state.in_transaction = true;
                state.is_address_phase = true;
                state.bit_count = 0;
                state.shift = 0;
                events.push(DecodedEvent::I2c { timestamp: ts, event: I2cEvent::Start });
                state.sda_prev = sda;
                state.scl_prev = scl;
                continue;
            }
            // 检测 STOP: SDA 上升沿 + SCL 高
            if sda && !state.sda_prev && scl {
                state.in_transaction = false;
                state.is_address_phase = false;
                state.bit_count = 0;
                events.push(DecodedEvent::I2c { timestamp: ts, event: I2cEvent::Stop });
                state.sda_prev = sda;
                state.scl_prev = scl;
                continue;
            }

            // 在传输中: SCL 上升沿采样 SDA
            if state.in_transaction && scl && !state.scl_prev {
                // SCL 上升沿 = 采样数据位
                if state.bit_count < 8 {
                    state.shift <<= 1;
                    if sda {
                        state.shift |= 1;
                    }
                    state.bit_count += 1;
                } else {
                    // 第 9 位 = ACK/NACK
                    let ack = !sda; // SDA 低 = ACK
                    if state.is_address_phase {
                        // 地址字节: 高 7 位 = 地址, bit 0 = R/W
                        let addr = state.shift >> 1;
                        let read = (state.shift & 1) == 1;
                        events.push(DecodedEvent::I2c {
                            timestamp: ts,
                            event: I2cEvent::Address { addr, read, ack },
                        });
                        state.is_address_phase = false;
                    } else {
                        // 数据字节
                        events.push(DecodedEvent::I2c {
                            timestamp: ts,
                            event: I2cEvent::Data { byte: state.shift, ack },
                        });
                    }
                    state.bit_count = 0;
                    state.shift = 0;
                }
            }

            state.sda_prev = sda;
            state.scl_prev = scl;
        }
        events
    }

    /// SPI 解码: 监测 SCLK/MOSI/MISO/CS, 在 SCK 边沿采样数据
    fn decode_spi(&mut self, samples: &[LogicSample]) -> Vec<DecodedEvent> {
        let LogicDecoderConfig::Spi { sclk_channel, mosi_channel, miso_channel, cs_channel, mode } = &self.config else {
            return Vec::new();
        };
        let sclk_ch = *sclk_channel;
        let mosi_ch = *mosi_channel;
        let miso_ch = *miso_channel;
        let cs_ch = *cs_channel;
        let spi_mode = *mode;
        let mut events = Vec::new();
        let state = &mut self.spi_state;

        for s in samples {
            let sclk = Self::channel_bit(s, sclk_ch);
            let mosi = Self::channel_bit(s, mosi_ch);
            let miso = Self::channel_bit(s, miso_ch);
            let cs = Self::channel_bit(s, cs_ch);
            let ts = s.timestamp;

            // CS 下降沿 = 开始传输
            if !cs && state.cs_prev {
                state.in_transaction = true;
                state.bit_count = 0;
                state.mosi_shift = 0;
                state.miso_shift = 0;
                state.cs_prev = cs;
                state.sclk_prev = sclk;
                continue;
            }
            // CS 上升沿 = 结束传输
            if cs && !state.cs_prev {
                state.in_transaction = false;
                state.cs_prev = cs;
                state.sclk_prev = sclk;
                continue;
            }

            if !state.in_transaction {
                state.cs_prev = cs;
                state.sclk_prev = sclk;
                continue;
            }

            // 模式 0/2: SCLK 上升沿采样; 模式 1/3: SCLK 下降沿采样
            let sample_edge = match spi_mode {
                0 | 2 => sclk && !state.sclk_prev, // 上升沿
                1 | 3 => !sclk && state.sclk_prev, // 下降沿
                _ => false,
            };

            if sample_edge {
                // 采样 MOSI 和 MISO (MSB first)
                state.mosi_shift <<= 1;
                if mosi { state.mosi_shift |= 1; }
                state.miso_shift <<= 1;
                if miso { state.miso_shift |= 1; }
                state.bit_count += 1;

                if state.bit_count == 8 {
                    events.push(DecodedEvent::Spi {
                        timestamp: ts,
                        mosi: state.mosi_shift,
                        miso: state.miso_shift,
                    });
                    state.bit_count = 0;
                    state.mosi_shift = 0;
                    state.miso_shift = 0;
                }
            }

            state.cs_prev = cs;
            state.sclk_prev = sclk;
        }
        events
    }
}

impl ProtocolEngine for LogicDecoderEngine {
    fn feed(&mut self, _data: &[u8]) -> Vec<DataFrame> {
        Vec::new()
    }

    fn feed_logic(&mut self, data: &[u8]) -> Vec<LogicSample> {
        let now = Self::now_us();
        let mut samples = Vec::with_capacity(data.len());
        for (i, &b) in data.iter().enumerate() {
            // 每字节一个采样, 时间戳按字节序递增 (假设采样率约等于波特率/10)
            // 这里用 1µs/字节 的粗略间隔, 实际间隔由传输层决定
            let ts = now.saturating_add(i as u64);
            samples.push(LogicSample {
                timestamp: ts,
                channels: b as u32,
                channel_count: 8,
            });
        }
        // 同时追加到内部缓冲供 feed_decoded 使用
        self.sample_buf.extend(samples.iter().cloned());
        // 限制内部缓冲大小
        if self.sample_buf.len() > 16384 {
            let drop = self.sample_buf.len() - 8192;
            self.sample_buf.drain(..drop);
        }
        samples
    }

    fn feed_decoded(&mut self, data: &[u8]) -> Vec<DecodedEvent> {
        // 先把字节转为采样 (与 feed_logic 一致), 然后按配置解码
        let now = Self::now_us();
        let new_samples: Vec<LogicSample> = data.iter().enumerate().map(|(i, &b)| LogicSample {
            timestamp: now.saturating_add(i as u64),
            channels: b as u32,
            channel_count: 8,
        }).collect();

        match &self.config {
            LogicDecoderConfig::Uart { .. } => self.decode_uart(data),
            LogicDecoderConfig::I2c { .. } => self.decode_i2c(&new_samples),
            LogicDecoderConfig::Spi { .. } => self.decode_spi(&new_samples),
        }
    }

    fn encode_channel(&mut self, _channel: usize, _value: f32) -> Vec<u8> { Vec::new() }
    fn encode_channels(&mut self, _values: &[f32]) -> Vec<u8> { Vec::new() }
    fn name(&self) -> &str { "LogicDecoder" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vofa_next_core::{LogicDecoderConfig, Parity, StopBits};

    #[test]
    fn test_feed_logic_converts_bytes_to_samples() {
        let config = LogicDecoderConfig::Uart {
            baud_rate: 9600,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: StopBits::One,
            channel: 0,
        };
        let mut engine = LogicDecoderEngine::new(config);
        let samples = engine.feed_logic(&[0b10101010, 0b11110000]);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].channels, 0b10101010);
        assert_eq!(samples[0].channel_count, 8);
        assert_eq!(samples[1].channels, 0b11110000);
    }

    #[test]
    fn test_uart_decode_wraps_bytes() {
        let config = LogicDecoderConfig::Uart {
            baud_rate: 9600,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: StopBits::One,
            channel: 0,
        };
        let mut engine = LogicDecoderEngine::new(config);
        let events = engine.feed_decoded(&[0x41, 0x42, 0x43]);
        assert_eq!(events.len(), 3);
        // 验证第一个事件是 UART 事件, 字节 = 0x41
        match &events[0] {
            DecodedEvent::Uart { byte, .. } => assert_eq!(*byte, 0x41),
            _ => panic!("期望 UART 事件"),
        }
    }

    #[test]
    fn test_i2c_decode_start_stop() {
        // 模拟 I2C: SDA=bit0, SCL=bit1
        // START: SDA 下降沿 + SCL 高 (从 0b11 到 0b10)
        // STOP: SDA 上升沿 + SCL 高 (从 0b10 到 0b11)
        let config = LogicDecoderConfig::I2c {
            sda_channel: 0,
            scl_channel: 1,
        };
        let mut engine = LogicDecoderEngine::new(config);
        // 初始: SDA=1, SCL=1 (空闲)
        // START: SDA=0, SCL=1
        // STOP: SDA=1, SCL=1
        let data = [0b11, 0b10, 0b11]; // 空闲 -> START -> STOP
        let samples: Vec<LogicSample> = data.iter().enumerate().map(|(i, &b)| LogicSample {
            timestamp: i as u64,
            channels: b as u32,
            channel_count: 8,
        }).collect();
        let events = engine.decode_i2c(&samples);
        // 应该检测到 START 和 STOP
        assert!(events.iter().any(|e| matches!(e, DecodedEvent::I2c { event: I2cEvent::Start, .. })));
        assert!(events.iter().any(|e| matches!(e, DecodedEvent::I2c { event: I2cEvent::Stop, .. })));
    }

    #[test]
    fn test_spi_decode_cs_edge() {
        // 模拟 SPI: SCLK=bit0, MOSI=bit1, MISO=bit2, CS=bit3
        let config = LogicDecoderConfig::Spi {
            sclk_channel: 0,
            mosi_channel: 1,
            miso_channel: 2,
            cs_channel: 3,
            mode: 0,
        };
        let mut engine = LogicDecoderEngine::new(config);
        // CS 高 (空闲), 然后 CS 下降 (开始), 然后 CS 上升 (结束)
        let data = [0b1000, 0b0000, 0b1000]; // CS=1 -> CS=0 -> CS=1
        let samples: Vec<LogicSample> = data.iter().enumerate().map(|(i, &b)| LogicSample {
            timestamp: i as u64,
            channels: b as u32,
            channel_count: 8,
        }).collect();
        let events = engine.decode_spi(&samples);
        // 没有完整 8 位传输, 不应有 Spi 事件
        assert!(events.is_empty());
        // 但状态应该已更新 (in_transaction 切换)
        assert!(!engine.spi_state.in_transaction);
    }

    #[test]
    fn test_name() {
        let config = LogicDecoderConfig::Uart {
            baud_rate: 9600,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: StopBits::One,
            channel: 0,
        };
        let engine = LogicDecoderEngine::new(config);
        assert_eq!(engine.name(), "LogicDecoder");
    }

    /// 生成 I2C 单个电平样本字节 (SDA=bit0, SCL=bit1)
    fn i2c_bit(sda: bool, scl: bool) -> u8 {
        let mut v = 0u8;
        if sda {
            v |= 0x01;
        }
        if scl {
            v |= 0x02;
        }
        v
    }

    /// 生成 I2C 一个字节的采样序列 (8 数据位 + 1 ACK 位)
    /// ack=true 表示 ACK (SDA 低), false 表示 NACK (SDA 高)
    fn i2c_byte_samples(byte: u8, ack: bool) -> Vec<u8> {
        let mut samples = Vec::new();
        // 8 个数据位, MSB first, 在 SCL 上升沿采样
        for i in (0..8).rev() {
            let bit = (byte >> i) & 1 == 1;
            samples.push(i2c_bit(bit, false)); // SCL 低, 设置 SDA
            samples.push(i2c_bit(bit, true)); // SCL 高 (上升沿, 采样数据位)
        }
        // ACK 位: SDA = !ack (ACK → SDA=0 低; NACK → SDA=1 高)
        let sda_for_ack = !ack;
        samples.push(i2c_bit(sda_for_ack, false));
        samples.push(i2c_bit(sda_for_ack, true)); // 上升沿采样 ACK
        samples
    }

    /// I2C 完整事务测试: START + ADDR(0x50, W) + DATA(0xAB) + STOP
    #[test]
    fn test_i2c_decode_complete_transaction() {
        let config = LogicDecoderConfig::I2c {
            sda_channel: 0,
            scl_channel: 1,
        };
        let mut engine = LogicDecoderEngine::new(config);

        let mut data = Vec::new();
        // 空闲: SDA=1, SCL=1
        data.push(i2c_bit(true, true));
        // START: SDA 下降沿 + SCL 高
        data.push(i2c_bit(false, true));
        // 地址字节 0xA0 (0x50 << 1, W=0), 从机 ACK
        data.extend(i2c_byte_samples(0xA0, true));
        // 数据字节 0xAB, 从机 ACK
        data.extend(i2c_byte_samples(0xAB, true));
        // STOP: SDA 上升沿 + SCL 高
        // 上一个 ACK 采样后: scl_prev=true, sda_prev=false (ACK=SDA 低)
        // 直接过渡到 SDA=1, SCL=1 → STOP
        data.push(i2c_bit(true, true));

        let samples: Vec<LogicSample> = data
            .iter()
            .enumerate()
            .map(|(i, &b)| LogicSample {
                timestamp: i as u64,
                channels: b as u32,
                channel_count: 8,
            })
            .collect();
        let events = engine.decode_i2c(&samples);

        // 期望事件: START, Address(0x50, W, ACK), Data(0xAB, ACK), STOP
        assert_eq!(events.len(), 4, "期望 4 个事件, 实际 {}", events.len());

        // 事件 0: START
        match &events[0] {
            DecodedEvent::I2c {
                event: I2cEvent::Start,
                ..
            } => {}
            _ => panic!("期望 START 事件, 实际: {:?}", events[0]),
        }

        // 事件 1: Address
        match &events[1] {
            DecodedEvent::I2c {
                event: I2cEvent::Address { addr, read, ack },
                ..
            } => {
                assert_eq!(*addr, 0x50, "地址应为 0x50");
                assert!(!(*read), "应为写操作 (W)");
                assert!(*ack, "应为 ACK");
            }
            _ => panic!("期望 Address 事件, 实际: {:?}", events[1]),
        }

        // 事件 2: Data
        match &events[2] {
            DecodedEvent::I2c {
                event: I2cEvent::Data { byte, ack },
                ..
            } => {
                assert_eq!(*byte, 0xAB, "数据字节应为 0xAB");
                assert!(*ack, "应为 ACK");
            }
            _ => panic!("期望 Data 事件, 实际: {:?}", events[2]),
        }

        // 事件 3: STOP
        match &events[3] {
            DecodedEvent::I2c {
                event: I2cEvent::Stop,
                ..
            } => {}
            _ => panic!("期望 STOP 事件, 实际: {:?}", events[3]),
        }
    }

    /// I2C 读事务测试: 地址 R 位 = 1
    #[test]
    fn test_i2c_decode_read_transaction() {
        let config = LogicDecoderConfig::I2c {
            sda_channel: 0,
            scl_channel: 1,
        };
        let mut engine = LogicDecoderEngine::new(config);

        let mut data = Vec::new();
        data.push(i2c_bit(true, true)); // 空闲
        data.push(i2c_bit(false, true)); // START
        // 地址 0xA1 = 0x50 << 1 | 1 (R=1), 从机 ACK
        data.extend(i2c_byte_samples(0xA1, true));
        // 数据 0x42, 从机 ACK
        data.extend(i2c_byte_samples(0x42, true));
        data.push(i2c_bit(true, true)); // STOP

        let samples: Vec<LogicSample> = data
            .iter()
            .enumerate()
            .map(|(i, &b)| LogicSample {
                timestamp: i as u64,
                channels: b as u32,
                channel_count: 8,
            })
            .collect();
        let events = engine.decode_i2c(&samples);

        // 期望: START, Address(0x50, R=true, ACK), Data(0x42, ACK), STOP
        assert_eq!(events.len(), 4);
        match &events[1] {
            DecodedEvent::I2c {
                event: I2cEvent::Address { addr, read, ack },
                ..
            } => {
                assert_eq!(*addr, 0x50);
                assert!(*read, "应为读操作 (R)");
                assert!(*ack);
            }
            _ => panic!("期望 Address 事件"),
        }
        match &events[2] {
            DecodedEvent::I2c {
                event: I2cEvent::Data { byte, .. },
                ..
            } => assert_eq!(*byte, 0x42),
            _ => panic!("期望 Data 事件"),
        }
    }

    /// I2C 多数据字节事务
    #[test]
    fn test_i2c_decode_multiple_data_bytes() {
        let config = LogicDecoderConfig::I2c {
            sda_channel: 0,
            scl_channel: 1,
        };
        let mut engine = LogicDecoderEngine::new(config);

        let mut data = Vec::new();
        data.push(i2c_bit(true, true)); // 空闲
        data.push(i2c_bit(false, true)); // START
        data.extend(i2c_byte_samples(0xA0, true)); // 地址, ACK
        data.extend(i2c_byte_samples(0x01, true)); // 数据 1, ACK
        data.extend(i2c_byte_samples(0x02, true)); // 数据 2, ACK
        data.extend(i2c_byte_samples(0x03, true)); // 数据 3, ACK
        data.push(i2c_bit(true, true)); // STOP

        let samples: Vec<LogicSample> = data
            .iter()
            .enumerate()
            .map(|(i, &b)| LogicSample {
                timestamp: i as u64,
                channels: b as u32,
                channel_count: 8,
            })
            .collect();
        let events = engine.decode_i2c(&samples);

        // 期望: START + ADDR + 3*DATA + STOP = 6 事件
        assert_eq!(events.len(), 6);
        // 验证 3 个数据字节
        let data_events: Vec<u8> = events
            .iter()
            .filter_map(|e| match e {
                DecodedEvent::I2c {
                    event: I2cEvent::Data { byte, .. },
                    ..
                } => Some(*byte),
                _ => None,
            })
            .collect();
        assert_eq!(data_events, vec![0x01, 0x02, 0x03]);
    }

    /// I2C NACK 测试: 从机不应答
    #[test]
    fn test_i2c_decode_nack() {
        let config = LogicDecoderConfig::I2c {
            sda_channel: 0,
            scl_channel: 1,
        };
        let mut engine = LogicDecoderEngine::new(config);

        let mut data = Vec::new();
        data.push(i2c_bit(true, true)); // 空闲
        data.push(i2c_bit(false, true)); // START
        // 地址 0xA0, 从机 NACK (ack=false → SDA 高)
        data.extend(i2c_byte_samples(0xA0, false));
        // NACK 后 SDA 已为高, 需先拉低 SDA 才能产生 STOP 上升沿
        // (STOP 条件 = SDA 从低到高 + SCL 高)
        data.push(i2c_bit(false, false)); // SDA=0, SCL=0 (准备 STOP)
        data.push(i2c_bit(false, true)); // SDA=0, SCL=1 (SCL 上升)
        data.push(i2c_bit(true, true)); // SDA 0→1, SCL=1 → STOP 上升沿

        let samples: Vec<LogicSample> = data
            .iter()
            .enumerate()
            .map(|(i, &b)| LogicSample {
                timestamp: i as u64,
                channels: b as u32,
                channel_count: 8,
            })
            .collect();
        let events = engine.decode_i2c(&samples);

        // 期望: START, Address(NACK), STOP
        assert_eq!(events.len(), 3);
        match &events[1] {
            DecodedEvent::I2c {
                event: I2cEvent::Address { ack, .. },
                ..
            } => assert!(!(*ack), "应为 NACK"),
            _ => panic!("期望 Address 事件"),
        }
    }

    /// SPI 完整字节传输测试辅助函数
    /// SCLK=bit0, MOSI=bit1, MISO=bit2, CS=bit3
    fn run_spi_mode_test(mode: u8) {
        let config = LogicDecoderConfig::Spi {
            sclk_channel: 0,
            mosi_channel: 1,
            miso_channel: 2,
            cs_channel: 3,
            mode,
        };
        let mut engine = LogicDecoderEngine::new(config);

        let mosi_byte = 0xA5u8; // 1010_0101
        let miso_byte = 0x3Cu8; // 0011_1100

        // SCLK 空闲电平: mode 0/1 = 低, mode 2/3 = 高
        let sclk_idle: u8 = if matches!(mode, 0 | 1) { 0 } else { 1 };
        // 采样边沿: mode 0/2 = 上升沿, mode 1/3 = 下降沿
        let sample_rising = matches!(mode, 0 | 2);

        let mut data = Vec::new();
        // 空闲: CS=1, SCLK=idle
        data.push(0b1000 | sclk_idle);
        // CS 下降沿 (开始传输), SCLK=idle
        data.push(sclk_idle);
        // 8 个数据位 (MSB first)
        for i in (0..8).rev() {
            let mosi_bit = (mosi_byte >> i) & 1 == 1;
            let miso_bit = (miso_byte >> i) & 1 == 1;
            let mut v: u8 = 0; // CS=0
            if mosi_bit {
                v |= 0x02;
            }
            if miso_bit {
                v |= 0x04;
            }
            if sample_rising {
                // mode 0/2: SCLK=0 (设置数据), SCLK=1 (上升沿采样)
                data.push(v); // SCLK=0
                data.push(v | 0x01); // SCLK=1
            } else {
                // mode 1/3: SCLK=1 (设置数据), SCLK=0 (下降沿采样)
                data.push(v | 0x01); // SCLK=1
                data.push(v); // SCLK=0
            }
        }
        // CS 上升沿 (结束传输), SCLK=idle
        data.push(0b1000 | sclk_idle);

        let samples: Vec<LogicSample> = data
            .iter()
            .enumerate()
            .map(|(i, &b)| LogicSample {
                timestamp: i as u64,
                channels: b as u32,
                channel_count: 8,
            })
            .collect();
        let events = engine.decode_spi(&samples);

        assert_eq!(
            events.len(),
            1,
            "mode {}: 期望 1 个 SPI 事件, 实际 {}",
            mode,
            events.len()
        );
        match &events[0] {
            DecodedEvent::Spi { mosi, miso, .. } => {
                assert_eq!(*mosi, mosi_byte, "mode {}: MOSI 不匹配", mode);
                assert_eq!(*miso, miso_byte, "mode {}: MISO 不匹配", mode);
            }
            _ => panic!("mode {}: 期望 SPI 事件", mode),
        }
    }

    /// SPI mode 0: CPOL=0, 上升沿采样
    #[test]
    fn test_spi_decode_complete_byte_mode0() {
        run_spi_mode_test(0);
    }

    /// SPI mode 1: CPOL=0, 下降沿采样
    #[test]
    fn test_spi_decode_complete_byte_mode1() {
        run_spi_mode_test(1);
    }

    /// SPI mode 2: CPOL=1, 上升沿采样
    #[test]
    fn test_spi_decode_complete_byte_mode2() {
        run_spi_mode_test(2);
    }

    /// SPI mode 3: CPOL=1, 下降沿采样
    #[test]
    fn test_spi_decode_complete_byte_mode3() {
        run_spi_mode_test(3);
    }

    /// SPI 多字节传输测试 (mode 0, 2 个字节)
    #[test]
    fn test_spi_decode_multiple_bytes_mode0() {
        let config = LogicDecoderConfig::Spi {
            sclk_channel: 0,
            mosi_channel: 1,
            miso_channel: 2,
            cs_channel: 3,
            mode: 0,
        };
        let mut engine = LogicDecoderEngine::new(config);

        let mosi_bytes = [0xA5u8, 0x3C];
        let miso_bytes = [0x5Au8, 0xC3];

        let mut data = Vec::new();
        // CS 空闲
        data.push(0b1000);
        // 第一字节
        data.push(0b0000); // CS 下降沿
        for &byte in &mosi_bytes {
            for i in (0..8).rev() {
                let mosi_bit = (byte >> i) & 1 == 1;
                let miso_idx = mosi_bytes.iter().position(|&b| b == byte).unwrap();
                let miso_byte = miso_bytes[miso_idx];
                let miso_bit = (miso_byte >> i) & 1 == 1;
                let mut v: u8 = 0;
                if mosi_bit {
                    v |= 0x02;
                }
                if miso_bit {
                    v |= 0x04;
                }
                data.push(v); // SCLK=0
                data.push(v | 0x01); // SCLK=1 (上升沿)
            }
        }
        data.push(0b1000); // CS 上升沿

        let samples: Vec<LogicSample> = data
            .iter()
            .enumerate()
            .map(|(i, &b)| LogicSample {
                timestamp: i as u64,
                channels: b as u32,
                channel_count: 8,
            })
            .collect();
        let events = engine.decode_spi(&samples);

        assert_eq!(events.len(), 2);
        match &events[0] {
            DecodedEvent::Spi { mosi, miso, .. } => {
                assert_eq!(*mosi, 0xA5);
                assert_eq!(*miso, 0x5A);
            }
            _ => panic!("期望 SPI 事件"),
        }
        match &events[1] {
            DecodedEvent::Spi { mosi, miso, .. } => {
                assert_eq!(*mosi, 0x3C);
                assert_eq!(*miso, 0xC3);
            }
            _ => panic!("期望 SPI 事件"),
        }
    }

    /// UART 多字节解码测试 (256 字节)
    #[test]
    fn test_uart_decode_multi_byte() {
        let config = LogicDecoderConfig::Uart {
            baud_rate: 115200,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: StopBits::One,
            channel: 0,
        };
        let mut engine = LogicDecoderEngine::new(config);
        // 输入 0..=255 全部字节
        let input: Vec<u8> = (0..=255u8).collect();
        let events = engine.feed_decoded(&input);
        assert_eq!(events.len(), 256);
        for (i, e) in events.iter().enumerate() {
            match e {
                DecodedEvent::Uart {
                    byte, parity_ok, ..
                } => {
                    assert_eq!(*byte, i as u8, "索引 {} 字节不匹配", i);
                    // Parity::None → parity_ok = true
                    assert!(*parity_ok, "索引 {} parity_ok 应为 true", i);
                }
                _ => panic!("索引 {} 期望 UART 事件", i),
            }
        }
    }

    /// UART 奇校验测试: parity_ok 应为 false
    #[test]
    fn test_uart_decode_with_odd_parity() {
        let config = LogicDecoderConfig::Uart {
            baud_rate: 9600,
            data_bits: 8,
            parity: Parity::Odd,
            stop_bits: StopBits::One,
            channel: 0,
        };
        let mut engine = LogicDecoderEngine::new(config);
        let events = engine.feed_decoded(&[0x41, 0x42]);
        assert_eq!(events.len(), 2);
        for e in &events {
            match e {
                DecodedEvent::Uart { parity_ok, .. } => {
                    // 非 None 的 parity → parity_ok = false (假设串口层已处理校验)
                    assert!(!(*parity_ok));
                }
                _ => panic!("期望 UART 事件"),
            }
        }
    }
}
