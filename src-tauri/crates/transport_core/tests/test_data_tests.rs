//! 测试数据生成器字节格式验证
//!
//! 集成测试: 通过 `transport_core::test_data::generate_bytes` 验证每种
//! `ProtocolConfig` 变体生成的字节格式正确 (JustFloat 帧尾 / FireWater CSV /
//! Slcan ASCII / CandleLight 二进制 / RawData 计数器 / LogicDecode 字节位)。

use transport_core::test_data::generate_bytes;
use logic_types::LogicDecoderConfig;
use schema_types::ProtocolConfig;
use vofa_core::{Parity, StopBits, TestSignal};

#[test]
fn justfloat_format() {
    let protocol = ProtocolConfig::JustFloat { channels: Some(2) };
    let data = generate_bytes(2, TestSignal::Sine, 0.0, &protocol, 0);
    // 2 channels * 4 bytes + 4 byte tail
    assert_eq!(data.len(), 12);
    // 帧尾
    assert_eq!(&data[8..12], &[0x00, 0x00, 0x80, 0x7f]);
}

#[test]
fn firewater_format() {
    let protocol = ProtocolConfig::FireWater { channels: Some(2) };
    let data = generate_bytes(2, TestSignal::Sine, 0.0, &protocol, 0);
    let s = String::from_utf8(data).unwrap();
    assert!(s.ends_with('\n'));
    assert_eq!(s.matches(',').count(), 1);
}

#[test]
fn slcan_format() {
    let protocol = ProtocolConfig::Slcan;
    let data = generate_bytes(8, TestSignal::Square, 0.0, &protocol, 0);
    let s = String::from_utf8(data).unwrap();
    assert!(s.starts_with('t'));
    assert!(s.ends_with('\r'));
    // t + 3 (id) + 1 (dlc) + 16 (8 bytes hex) + 1 (\r) = 22
    assert_eq!(s.len(), 22);
}

#[test]
fn candle_format() {
    let protocol = ProtocolConfig::CandleLight;
    let data = generate_bytes(8, TestSignal::Square, 0.0, &protocol, 0);
    assert_eq!(data.len(), 24);
    assert_eq!(data[0], 0x11); // RX cmd
    assert_eq!(data[12], 8); // dlc
}

#[test]
fn rawdata_format() {
    let protocol = ProtocolConfig::RawData;
    let data = generate_bytes(4, TestSignal::Dc, 0.0, &protocol, 42);
    // 4 channel bytes + 4 byte counter
    assert_eq!(data.len(), 8);
    assert_eq!(&data[4..8], &42u32.to_le_bytes());
}

#[test]
fn logic_decode_format() {
    let protocol = ProtocolConfig::LogicDecode {
        decoder: LogicDecoderConfig::Uart {
            baud_rate: 115200,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: StopBits::One,
            channel: 0,
        },
    };
    let data = generate_bytes(8, TestSignal::Square, 0.0, &protocol, 0);
    // 8 samples per tick
    assert_eq!(data.len(), 8);
    // 通道 0 应有方波翻转
    assert_ne!(data[0] & 0x01, 0);
}