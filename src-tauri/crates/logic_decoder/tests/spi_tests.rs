//! 集成测试: SPI 解码

use logic_decoder::LogicDecoderEngine;
use logic_types::{DecodedEvent, LogicDecoderConfig, LogicSample};

fn to_samples(data: &[u8]) -> Vec<LogicSample> {
    data.iter()
        .enumerate()
        .map(|(i, &b)| LogicSample {
            timestamp: i as u64,
            channels: b as u32,
            channel_count: 8,
        })
        .collect()
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
    let samples = to_samples(&data);
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
    let samples = to_samples(&data);
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
    let samples = to_samples(&data);
    let events = engine.decode_spi(&samples);
    assert!(events.is_empty());
}