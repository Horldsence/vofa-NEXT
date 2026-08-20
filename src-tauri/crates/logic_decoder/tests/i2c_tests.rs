//! 集成测试: I2C 解码

use logic_decoder::LogicDecoderEngine;
use logic_types::{DecodedEvent, I2cEvent, LogicDecoderConfig, LogicSample};

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

#[test]
fn test_i2c_decode_start_stop() {
    let config = LogicDecoderConfig::I2c {
        sda_channel: 0,
        scl_channel: 1,
    };
    let mut engine = LogicDecoderEngine::new(config);
    let data = [0b11, 0b10, 0b11];
    let samples = to_samples(&data);
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
    let samples = to_samples(&data);
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
    let samples = to_samples(&data);
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
    let samples = to_samples(&data);
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
    let samples = to_samples(&data);
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