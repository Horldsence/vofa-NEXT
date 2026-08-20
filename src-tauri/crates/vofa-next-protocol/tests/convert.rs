//! 协议转换测试 — encode_frame 跨协议重编码 roundtrip
//!
//! 协议 A 编码 → 协议 B feed 解析 → 数值还原, 验证 encode_frame
//! 在手动/自动通道模式下均产出正确帧。

use vofa_next_core::DataFrame;
use vofa_next_protocol::{FireWaterEngine, JustFloatEngine, ProtocolEngine};

/// JustFloat → FireWater: JustFloat 字节流解析出 DataFrame 后,
/// 由 FireWater 重编码并解析还原, 验证跨协议转换数值一致。
#[test]
fn test_convert_justfloat_to_firewater() {
    let channels = vec![1.5, -2.25, 3.0];

    let mut jf_src = JustFloatEngine::new(Some(3));
    let bytes = jf_src.encode_frame(&DataFrame::new(channels.clone()));

    let mut jf_rx = JustFloatEngine::new(None);
    let frames = jf_rx.feed(&bytes).frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, channels);

    let mut fw_dst = FireWaterEngine::new(None);
    let out = fw_dst.encode_frame(&frames[0]);

    let mut fw_rx = FireWaterEngine::new(None);
    let restored = fw_rx.feed(&out).frames;
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].channels.len(), 3);
    for (a, b) in restored[0].channels.iter().zip(channels.iter()) {
        assert!((a - b).abs() < 1e-6, "expected {b}, got {a}");
    }
}

#[test]
fn test_convert_firewater_to_justfloat() {
    let channels = vec![0.5, 1.25];

    let mut fw_src = FireWaterEngine::new(Some(2));
    let bytes = fw_src.encode_frame(&DataFrame::new(channels.clone()));

    let mut fw_rx = FireWaterEngine::new(None);
    let frames = fw_rx.feed(&bytes).frames;
    assert_eq!(frames.len(), 1);

    let mut jf_dst = JustFloatEngine::new(None);
    let out = jf_dst.encode_frame(&frames[0]);

    let mut jf_rx = JustFloatEngine::new(None);
    let restored = jf_rx.feed(&out).frames;
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].channels.len(), 2);
    for (a, b) in restored[0].channels.iter().zip(channels.iter()) {
        assert!((a - b).abs() < 1e-6, "expected {b}, got {a}");
    }
}

#[test]
fn test_encode_frame_auto_mode_without_feed() {
    let frame = DataFrame::new(vec![1.0, 2.0, 3.0, 4.0]);

    let mut jf = JustFloatEngine::new(None);
    assert_eq!(jf.detected_channels(), None);
    let out = jf.encode_frame(&frame);
    assert_eq!(out.len(), 4 * 4 + 4);
    assert_eq!(jf.detected_channels(), Some(4));
    let single = jf.encode_channel(0, 9.0);
    assert_eq!(single.len(), 4 * 4 + 4);

    let mut fw = FireWaterEngine::new(None);
    assert_eq!(fw.detected_channels(), None);
    let out = fw.encode_frame(&frame);
    assert_eq!(fw.detected_channels(), Some(4));
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text, "1.000000,2.000000,3.000000,4.000000\n");
}

#[test]
fn test_encode_frame_semantic_mismatch_returns_empty() {
    use vofa_next_protocol::{CandleEngine, LogicDecoderEngine, RawDataEngine, SlcanEngine};

    let frame = DataFrame::new(vec![1.0, 2.0]);
    assert!(RawDataEngine::new().encode_frame(&frame).is_empty());
    assert!(SlcanEngine::new().encode_frame(&frame).is_empty());
    assert!(CandleEngine::new().encode_frame(&frame).is_empty());
    assert!(
        LogicDecoderEngine::new(vofa_next_core::LogicDecoderConfig::I2c {
            sda_channel: 0,
            scl_channel: 1,
        })
        .encode_frame(&frame)
        .is_empty()
    );
}

#[test]
fn test_create_engine_dispatch_all_variants() {
    use vofa_next_protocol::create_engine;

    // JustFloat
    let e = create_engine(&vofa_next_core::ProtocolConfig::JustFloat { channels: Some(2) });
    assert_eq!(e.name(), "JustFloat");
    drop(e);

    // FireWater
    let e = create_engine(&vofa_next_core::ProtocolConfig::FireWater { channels: None });
    assert_eq!(e.name(), "FireWater");
    drop(e);

    // RawData
    let e = create_engine(&vofa_next_core::ProtocolConfig::RawData);
    assert_eq!(e.name(), "RawData");
    drop(e);

    // Slcan
    let e = create_engine(&vofa_next_core::ProtocolConfig::Slcan);
    assert_eq!(e.name(), "Slcan");
    drop(e);

    // CandleLight
    let e = create_engine(&vofa_next_core::ProtocolConfig::CandleLight);
    assert_eq!(e.name(), "CandleLight");
    drop(e);

    // LogicDecode
    let e = create_engine(&vofa_next_core::ProtocolConfig::LogicDecode {
        decoder: vofa_next_core::LogicDecoderConfig::I2c {
            sda_channel: 0,
            scl_channel: 1,
        },
    });
    assert_eq!(e.name(), "LogicDecoder");
    drop(e);

    // Diagnostic → RawData 占位
    let e = create_engine(&vofa_next_core::ProtocolConfig::Diagnostic {
        config: vofa_next_core::DiagnosticConfig::default(),
    });
    assert_eq!(e.name(), "RawData");
}