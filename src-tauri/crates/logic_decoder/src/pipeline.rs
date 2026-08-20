//! UART / I2C / SPI 协议解码实现 — 各协议状态机

use logic_types::{DecodedEvent, I2cEvent, LogicDecoderConfig, LogicSample};

use crate::core::{I2cState, LogicDecoderEngine, SpiState, UartState};

impl LogicDecoderEngine {
    /// UART 解码: 串口接收的字节就是 UART 解码后的数据, 直接包装为 DecodedEvent
    pub fn decode_uart(&mut self, data: &[u8]) -> Vec<DecodedEvent> {
        let LogicDecoderConfig::Uart { parity, .. } = &self.config else {
            return Vec::new();
        };
        let now = Self::now_us();
        let state = &mut self.uart_state;
        let mut events = Vec::with_capacity(data.len());
        for &b in data {
            let ts = now;
            let parity_ok = matches!(parity, vofa_core::Parity::None);
            events.push(DecodedEvent::Uart {
                timestamp: ts,
                byte: b,
                parity_ok,
            });
            state.last_ts = ts;
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
        let _: &mut I2cState = state;
        let _: &mut UartState = &mut self.uart_state;
        let _: &mut SpiState = &mut self.spi_state;

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