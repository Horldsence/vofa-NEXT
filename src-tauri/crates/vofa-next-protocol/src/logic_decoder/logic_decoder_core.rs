//! 逻辑分析仪解码引擎核心实现

use vofa_next_core::{DecodedEvent, I2cEvent, LogicDecoderConfig, LogicSample};

use crate::engine::{FeedOutput, ProtocolEngine};

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
    pub spi_state: SpiState,
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
pub struct SpiState {
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
    pub in_transaction: bool,
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
            let ts = now;
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
    pub fn decode_i2c(&mut self, samples: &[LogicSample]) -> Vec<DecodedEvent> {
        let LogicDecoderConfig::I2c {
            sda_channel,
            scl_channel,
        } = &self.config
        else {
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
                events.push(DecodedEvent::I2c {
                    timestamp: ts,
                    event: I2cEvent::Start,
                });
                state.sda_prev = sda;
                state.scl_prev = scl;
                continue;
            }
            // 检测 STOP: SDA 上升沿 + SCL 高
            if sda && !state.sda_prev && scl {
                state.in_transaction = false;
                state.is_address_phase = false;
                state.bit_count = 0;
                events.push(DecodedEvent::I2c {
                    timestamp: ts,
                    event: I2cEvent::Stop,
                });
                state.sda_prev = sda;
                state.scl_prev = scl;
                continue;
            }

            // 在传输中: SCL 上升沿采样 SDA
            if state.in_transaction && scl && !state.scl_prev {
                if state.bit_count < 8 {
                    state.shift <<= 1;
                    if sda {
                        state.shift |= 1;
                    }
                    state.bit_count += 1;
                } else {
                    let ack = !sda;
                    if state.is_address_phase {
                        let addr = state.shift >> 1;
                        let read = (state.shift & 1) == 1;
                        events.push(DecodedEvent::I2c {
                            timestamp: ts,
                            event: I2cEvent::Address { addr, read, ack },
                        });
                        state.is_address_phase = false;
                    } else {
                        events.push(DecodedEvent::I2c {
                            timestamp: ts,
                            event: I2cEvent::Data {
                                byte: state.shift,
                                ack,
                            },
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
    pub fn decode_spi(&mut self, samples: &[LogicSample]) -> Vec<DecodedEvent> {
        let LogicDecoderConfig::Spi {
            sclk_channel,
            mosi_channel,
            miso_channel,
            cs_channel,
            mode,
        } = &self.config
        else {
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
                0 | 2 => sclk && !state.sclk_prev,
                1 | 3 => !sclk && state.sclk_prev,
                _ => false,
            };

            if sample_edge {
                state.mosi_shift <<= 1;
                if mosi {
                    state.mosi_shift |= 1;
                }
                state.miso_shift <<= 1;
                if miso {
                    state.miso_shift |= 1;
                }
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
    fn feed(&mut self, data: &[u8]) -> FeedOutput {
        let now = Self::now_us();
        let samples: Vec<LogicSample> = data
            .iter()
            .enumerate()
            .map(|(i, &b)| LogicSample {
                timestamp: now.saturating_add(i as u64),
                channels: b as u32,
                channel_count: 8,
            })
            .collect();
        self.sample_buf.extend(samples.iter().cloned());
        if self.sample_buf.len() > 16384 {
            let drop = self.sample_buf.len() - 8192;
            self.sample_buf.drain(..drop);
        }
        let decoded_events = match &self.config {
            LogicDecoderConfig::Uart { .. } => self.decode_uart(data),
            LogicDecoderConfig::I2c { .. } => self.decode_i2c(&samples),
            LogicDecoderConfig::Spi { .. } => self.decode_spi(&samples),
        };
        FeedOutput {
            logic_samples: samples,
            decoded_events,
            ..Default::default()
        }
    }

    fn encode_channel(&mut self, _channel: usize, _value: f32) -> Vec<u8> {
        Vec::new()
    }
    fn encode_channels(&mut self, _values: &[f32]) -> Vec<u8> {
        Vec::new()
    }
    fn name(&self) -> &str {
        "LogicDecoder"
    }

    fn new_worker(&self) -> Box<dyn ProtocolEngine> {
        Box::new(LogicDecoderEngine::new(self.config.clone()))
    }
}
