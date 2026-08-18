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
    // JustFloat 编码 → FireWater feed 解析
    // FireWater 是 ASCII 协议, JustFloat 输出为二进制,
    // 转换路径应为: JustFloat 解析出的 DataFrame → FireWater encode_frame。
    // 本测试验证该方向: 源帧 → JustFloat 编码 → JustFloat 解析出帧 → FireWater 重编码 → FireWater 解析还原。
    let channels = vec![1.5, -2.25, 3.0];

    // 源侧: JustFloat 编码字节流
    let mut jf_src = JustFloatEngine::new(Some(3));
    let bytes = jf_src.encode_frame(&DataFrame::new(channels.clone()));

    // JustFloat 解析回 DataFrame (模拟从总线收到 JustFloat 数据)
    let mut jf_rx = JustFloatEngine::new(None);
    let frames = jf_rx.feed(&bytes).frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, channels);

    // 目标侧: FireWater 重编码 (自动通道模式, 未 feed 过)
    let mut fw_dst = FireWaterEngine::new(None);
    let out = fw_dst.encode_frame(&frames[0]);

    // FireWater 解析还原数值
    let mut fw_rx = FireWaterEngine::new(None);
    let restored = fw_rx.feed(&out).frames;
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].channels.len(), 3);
    for (a, b) in restored[0].channels.iter().zip(channels.iter()) {
        // FireWater 编码保留 6 位小数
        assert!((a - b).abs() < 1e-6, "expected {b}, got {a}");
    }
}

#[test]
fn test_convert_firewater_to_justfloat() {
    // 反向: FireWater 编码 → FireWater 解析出帧 → JustFloat 重编码 → JustFloat 解析还原
    let channels = vec![0.5, 1.25];

    let mut fw_src = FireWaterEngine::new(Some(2));
    let bytes = fw_src.encode_frame(&DataFrame::new(channels.clone()));

    let mut fw_rx = FireWaterEngine::new(None);
    let frames = fw_rx.feed(&bytes).frames;
    assert_eq!(frames.len(), 1);

    // 目标侧: JustFloat 重编码 (自动通道模式, 未 feed 过)
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
    // 自动通道模式 (channels=None) 且从未 feed: encode_frame 应以输入帧通道数为准
    let frame = DataFrame::new(vec![1.0, 2.0, 3.0, 4.0]);

    // JustFloat: 4 通道 × 4 字节 + 4 字节帧尾
    let mut jf = JustFloatEngine::new(None);
    assert_eq!(jf.detected_channels(), None);
    let out = jf.encode_frame(&frame);
    assert_eq!(out.len(), 4 * 4 + 4);
    // 编码后 detected 通道数同步为 4, 后续 encode_channel 不再降级为 1 通道
    assert_eq!(jf.detected_channels(), Some(4));
    let single = jf.encode_channel(0, 9.0);
    assert_eq!(single.len(), 4 * 4 + 4);

    // FireWater: "1.000000,2.000000,3.000000,4.000000\n"
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
