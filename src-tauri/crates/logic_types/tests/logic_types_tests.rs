//! `logic_types::types` 单元测试
//!
//! 覆盖:
//! - `LogicSample` 构造与 `channel()` 读取
//! - `LogicBatch` 推入 / 状态
//! - `DecodedEvent.kind()` / `.timestamp()`
//! - `LogicSampleFilter.matches` 多场景
//! - `DecodedEventFilter.matches` (kind 子串 / payload 子串)
//! - `LogicDecoderConfig.kind()` 与 serde round-trip

use logic_types::{
    DecodedEvent, DecodedEventFilter, I2cEvent, LogicBatch, LogicDecoderConfig, LogicSample,
    LogicSampleBatch, LogicSampleFilter,
};
use vofa_core::{Parity, StopBits};

#[test]
fn logic_sample_channel_bit_read() {
    let s = LogicSample::new(0, 0b1010_0101, 8);
    assert!(s.channel(0));
    assert!(!s.channel(1));
    assert!(s.channel(2));
    assert!(!s.channel(3));
    assert!(!s.channel(4));
    assert!(s.channel(5));
    assert!(!s.channel(6));
    assert!(s.channel(7));
}

#[test]
fn logic_batch_push_and_len() {
    let mut b = LogicBatch::new();
    assert!(b.is_empty());
    assert_eq!(b.len(), 0);
    b.push(LogicSample::new(0, 0xFF, 8));
    b.push(LogicSample::new(100, 0x00, 8));
    assert_eq!(b.len(), 2);
}

#[test]
fn decoded_event_kind_and_timestamp() {
    let uart = DecodedEvent::Uart {
        timestamp: 100,
        byte: 0x42,
        parity_ok: true,
    };
    assert_eq!(uart.kind(), "uart");
    assert_eq!(uart.timestamp(), 100);

    let i2c = DecodedEvent::I2c {
        timestamp: 200,
        event: I2cEvent::Start,
    };
    assert_eq!(i2c.kind(), "i2c");
    assert_eq!(i2c.timestamp(), 200);

    let spi = DecodedEvent::Spi {
        timestamp: 300,
        mosi: 0xAA,
        miso: 0x55,
    };
    assert_eq!(spi.kind(), "spi");
    assert_eq!(spi.timestamp(), 300);
}

#[test]
fn sample_filter_empty_matches_all() {
    let f = LogicSampleFilter::default();
    assert!(f.is_empty());
    let s = LogicSample::new(0, 0xFF, 8);
    assert!(f.matches(&s));
}

#[test]
fn sample_filter_mask_matches_value() {
    let f = LogicSampleFilter {
        channel_mask: Some(0b0000_1111),
        channel_value: Some(0b0000_0101),
    };
    // channel bits 0..3 = 0101 → 匹配
    assert!(f.matches(&LogicSample::new(0, 0b1010_0101, 8)));
    // channel bits 0..3 = 0010 → 不匹配
    assert!(!f.matches(&LogicSample::new(0, 0b1010_0010, 8)));
}

#[test]
fn sample_filter_value_defaults_to_zero() {
    let f = LogicSampleFilter {
        channel_mask: Some(0b1000_0000),
        channel_value: None,
    };
    // bit 7 = 0 → 匹配默认 value=0
    assert!(f.matches(&LogicSample::new(0, 0b0000_0000, 8)));
    // bit 7 = 1 → 不匹配
    assert!(!f.matches(&LogicSample::new(0, 0b1000_0000, 8)));
}

#[test]
fn event_filter_kind_case_insensitive() {
    let f = DecodedEventFilter {
        kind: Some("UART".into()),
        byte_pattern: None,
    };
    let evt = DecodedEvent::Uart {
        timestamp: 0,
        byte: 0,
        parity_ok: true,
    };
    assert!(f.matches(&evt));
}

#[test]
fn event_filter_kind_rejects_other_protocols() {
    let f = DecodedEventFilter {
        kind: Some("uart".into()),
        byte_pattern: None,
    };
    let i2c = DecodedEvent::I2c {
        timestamp: 0,
        event: I2cEvent::Stop,
    };
    assert!(!f.matches(&i2c));
}

#[test]
fn event_filter_byte_pattern_substring() {
    let f = DecodedEventFilter {
        kind: None,
        byte_pattern: Some(vec![0xAA, 0xBB]),
    };
    // Spi payload=[mosi, miso]=[0xAA, 0xBB] → 匹配
    assert!(f.matches(&DecodedEvent::Spi {
        timestamp: 0,
        mosi: 0xAA,
        miso: 0xBB,
    }));
    // Spi payload=[0xCC, 0xDD] → 不匹配
    assert!(!f.matches(&DecodedEvent::Spi {
        timestamp: 0,
        mosi: 0xCC,
        miso: 0xDD,
    }));
}

#[test]
fn event_filter_byte_pattern_empty_pattern_passes_all() {
    let f = DecodedEventFilter {
        kind: None,
        byte_pattern: Some(vec![]),
    };
    assert!(f.matches(&DecodedEvent::Uart {
        timestamp: 0,
        byte: 0,
        parity_ok: true,
    }));
}

#[test]
fn event_filter_payload_for_i2c_variants() {
    let f_addr = DecodedEventFilter {
        kind: None,
        byte_pattern: Some(vec![0x50]),
    };
    assert!(f_addr.matches(&DecodedEvent::I2c {
        timestamp: 0,
        event: I2cEvent::Address {
            addr: 0x50,
            read: true,
            ack: true,
        },
    }));

    let f_data = DecodedEventFilter {
        kind: None,
        byte_pattern: Some(vec![0x33]),
    };
    assert!(f_data.matches(&DecodedEvent::I2c {
        timestamp: 0,
        event: I2cEvent::Data {
            byte: 0x33,
            ack: true,
        },
    }));

    let f_none = DecodedEventFilter {
        kind: None,
        byte_pattern: Some(vec![0x01]),
    };
    // Start/Stop payload 为空,模式非空必不匹配
    assert!(!f_none.matches(&DecodedEvent::I2c {
        timestamp: 0,
        event: I2cEvent::Start,
    }));
}

#[test]
fn decoder_config_kind() {
    let uart = LogicDecoderConfig::Uart {
        baud_rate: 115_200,
        data_bits: 8,
        parity: Parity::None,
        stop_bits: StopBits::One,
        channel: 0,
    };
    assert_eq!(uart.kind(), "uart");

    let i2c = LogicDecoderConfig::I2c {
        sda_channel: 0,
        scl_channel: 1,
    };
    assert_eq!(i2c.kind(), "i2c");

    let spi = LogicDecoderConfig::Spi {
        sclk_channel: 2,
        mosi_channel: 3,
        miso_channel: 4,
        cs_channel: 5,
        mode: 0,
    };
    assert_eq!(spi.kind(), "spi");
}

#[test]
fn decoder_config_serde_roundtrip() {
    let cfg = LogicDecoderConfig::Uart {
        baud_rate: 9600,
        data_bits: 8,
        parity: Parity::Even,
        stop_bits: StopBits::Two,
        channel: 3,
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let restored: LogicDecoderConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, cfg);
}

#[test]
fn logic_sample_batch_default_seq_is_zero() {
    let b = LogicSampleBatch::default();
    assert_eq!(b.seq, 0);
    assert!(b.samples.is_empty());
}

#[test]
fn decoded_event_batch_serde_preserves_seq() {
    let mut b = logic_types::DecodedEventBatch::default();
    b.seq = 42;
    b.events.push(DecodedEvent::Uart {
        timestamp: 1,
        byte: 0x42,
        parity_ok: true,
    });
    let json = serde_json::to_string(&b).unwrap();
    let restored: logic_types::DecodedEventBatch = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.seq, 42);
    assert_eq!(restored.events.len(), 1);
}
