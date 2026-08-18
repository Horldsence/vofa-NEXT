//! 逻辑分析仪解码引擎
//!
//! - `logic_decoder_core`: 引擎核心实现 (状态机 + ProtocolEngine trait)

pub mod logic_decoder_core;

pub use logic_decoder_core::LogicDecoderEngine;

#[cfg(test)]
mod tests {
    use super::logic_decoder_core::LogicDecoderEngine;
    use crate::engine::ProtocolEngine;
    use vofa_next_core::{
        DecodedEvent, I2cEvent, LogicDecoderConfig, LogicSample, Parity, StopBits,
    };

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
        let samples = engine.feed(&[0b10101010, 0b11110000]).logic_samples;
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
        let events = engine.feed(&[0x41, 0x42, 0x43]).decoded_events;
        assert_eq!(events.len(), 3);
        match &events[0] {
            DecodedEvent::Uart { byte, .. } => assert_eq!(*byte, 0x41),
            _ => panic!("期望 UART 事件"),
        }
    }

    #[test]
    fn test_i2c_decode_start_stop() {
        let config = LogicDecoderConfig::I2c {
            sda_channel: 0,
            scl_channel: 1,
        };
        let mut engine = LogicDecoderEngine::new(config);
        let data = [0b11, 0b10, 0b11];
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
        assert!(events.iter().any(|e| matches!(
            e,
            DecodedEvent::I2c {
                event: I2cEvent::Start,
                ..
            }
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            DecodedEvent::I2c {
                event: I2cEvent::Stop,
                ..
            }
        )));
    }

    #[test]
    fn test_spi_decode_cs_edge() {
        let config = LogicDecoderConfig::Spi {
            sclk_channel: 0,
            mosi_channel: 1,
            miso_channel: 2,
            cs_channel: 3,
            mode: 0,
        };
        let mut engine = LogicDecoderEngine::new(config);
        let data = [0b1000, 0b0000, 0b1000];
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
        assert!(events.is_empty());
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

    fn i2c_byte_samples(byte: u8, ack: bool) -> Vec<u8> {
        let mut samples = Vec::new();
        for i in (0..8).rev() {
            let bit = (byte >> i) & 1 == 1;
            samples.push(i2c_bit(bit, false));
            samples.push(i2c_bit(bit, true));
        }
        let sda_for_ack = !ack;
        samples.push(i2c_bit(sda_for_ack, false));
        samples.push(i2c_bit(sda_for_ack, true));
        samples
    }

    #[test]
    fn test_i2c_decode_complete_transaction() {
        let config = LogicDecoderConfig::I2c {
            sda_channel: 0,
            scl_channel: 1,
        };
        let mut engine = LogicDecoderEngine::new(config);
        let mut data = Vec::new();
        data.push(i2c_bit(true, true));
        data.push(i2c_bit(false, true));
        data.extend(i2c_byte_samples(0xA0, true));
        data.extend(i2c_byte_samples(0xAB, true));
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
        assert_eq!(events.len(), 4, "期望 4 个事件, 实际 {}", events.len());
        match &events[1] {
            DecodedEvent::I2c {
                event: I2cEvent::Address { addr, read, ack },
                ..
            } => {
                assert_eq!(*addr, 0x50, "地址应为 0x50");
                assert!(!*read, "应为写操作 (W)");
                assert!(*ack, "应为 ACK");
            }
            _ => panic!("期望 Address 事件, 实际: {:?}", events[1]),
        }
    }

    #[test]
    fn test_i2c_decode_read_transaction() {
        let config = LogicDecoderConfig::I2c {
            sda_channel: 0,
            scl_channel: 1,
        };
        let mut engine = LogicDecoderEngine::new(config);
        let mut data = Vec::new();
        data.push(i2c_bit(true, true));
        data.push(i2c_bit(false, true));
        data.extend(i2c_byte_samples(0xA1, true));
        data.extend(i2c_byte_samples(0x42, true));
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
    }

    #[test]
    fn test_i2c_decode_multiple_data_bytes() {
        let config = LogicDecoderConfig::I2c {
            sda_channel: 0,
            scl_channel: 1,
        };
        let mut engine = LogicDecoderEngine::new(config);
        let mut data = Vec::new();
        data.push(i2c_bit(true, true));
        data.push(i2c_bit(false, true));
        data.extend(i2c_byte_samples(0xA0, true));
        data.extend(i2c_byte_samples(0x01, true));
        data.extend(i2c_byte_samples(0x02, true));
        data.extend(i2c_byte_samples(0x03, true));
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
        assert_eq!(events.len(), 6);
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

    #[test]
    fn test_i2c_decode_nack() {
        let config = LogicDecoderConfig::I2c {
            sda_channel: 0,
            scl_channel: 1,
        };
        let mut engine = LogicDecoderEngine::new(config);
        let mut data = Vec::new();
        data.push(i2c_bit(true, true));
        data.push(i2c_bit(false, true));
        data.extend(i2c_byte_samples(0xA0, false));
        data.push(i2c_bit(false, false));
        data.push(i2c_bit(false, true));
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
        assert_eq!(events.len(), 3);
        match &events[1] {
            DecodedEvent::I2c {
                event: I2cEvent::Address { ack, .. },
                ..
            } => assert!(!*ack, "应为 NACK"),
            _ => panic!("期望 Address 事件"),
        }
    }

    fn run_spi_mode_test(mode: u8) {
        let config = LogicDecoderConfig::Spi {
            sclk_channel: 0,
            mosi_channel: 1,
            miso_channel: 2,
            cs_channel: 3,
            mode,
        };
        let mut engine = LogicDecoderEngine::new(config);
        let mosi_byte = 0xA5u8;
        let miso_byte = 0x3Cu8;
        let sclk_idle: u8 = if matches!(mode, 0 | 1) { 0 } else { 1 };
        let sample_rising = matches!(mode, 0 | 2);
        let mut data = Vec::new();
        data.push(0b1000 | sclk_idle);
        data.push(sclk_idle);
        for i in (0..8).rev() {
            let mosi_bit = (mosi_byte >> i) & 1 == 1;
            let miso_bit = (miso_byte >> i) & 1 == 1;
            let mut v: u8 = 0;
            if mosi_bit {
                v |= 0x02;
            }
            if miso_bit {
                v |= 0x04;
            }
            if sample_rising {
                data.push(v);
                data.push(v | 0x01);
            } else {
                data.push(v | 0x01);
                data.push(v);
            }
        }
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

    #[test]
    fn test_spi_decode_complete_byte_mode0() {
        run_spi_mode_test(0);
    }
    #[test]
    fn test_spi_decode_complete_byte_mode1() {
        run_spi_mode_test(1);
    }
    #[test]
    fn test_spi_decode_complete_byte_mode2() {
        run_spi_mode_test(2);
    }
    #[test]
    fn test_spi_decode_complete_byte_mode3() {
        run_spi_mode_test(3);
    }

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
        data.push(0b1000);
        data.push(0b0000);
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
                data.push(v);
                data.push(v | 0x01);
            }
        }
        data.push(0b1000);
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
        let input: Vec<u8> = (0..=255u8).collect();
        let events = engine.feed(&input).decoded_events;
        assert_eq!(events.len(), 256);
        for (i, e) in events.iter().enumerate() {
            match e {
                DecodedEvent::Uart {
                    byte, parity_ok, ..
                } => {
                    assert_eq!(*byte, i as u8, "索引 {} 字节不匹配", i);
                    assert!(*parity_ok, "索引 {} parity_ok 应为 true", i);
                }
                _ => panic!("索引 {} 期望 UART 事件", i),
            }
        }
    }

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
        let events = engine.feed(&[0x41, 0x42]).decoded_events;
        assert_eq!(events.len(), 2);
        for e in &events {
            match e {
                DecodedEvent::Uart { parity_ok, .. } => {
                    assert!(!*parity_ok);
                }
                _ => panic!("期望 UART 事件"),
            }
        }
    }
}
