//! 集成测试: UART 解码

use logic_decoder::LogicDecoderEngine;
use logic_types::{DecodedEvent, LogicDecoderConfig};
use protocol_engine::ProtocolEngine;
use vofa_core::{Parity, StopBits};

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