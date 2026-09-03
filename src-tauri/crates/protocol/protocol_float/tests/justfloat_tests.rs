//! 集成测试: JustFloat 协议引擎

use protocol_engine::{FeedOutput, ProtocolEngine};
use protocol_float::JustFloatEngine;
use vofa_core::DataFrame;

/// JustFloat 帧尾: 0x00 0x00 0x80 0x7f (LE +Inf)
const TAIL: [u8; 4] = [0x00, 0x00, 0x80, 0x7f];

#[test]
fn test_parse_justfloat() {
    let mut engine = JustFloatEngine::new(Some(2));
    let mut data = Vec::new();
    data.extend_from_slice(&1.0_f32.to_le_bytes());
    data.extend_from_slice(&2.0_f32.to_le_bytes());
    data.extend_from_slice(&TAIL);

    let frames = engine.feed(&data).frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![1.0, 2.0]);
}

#[test]
fn test_parse_partial() {
    let mut engine = JustFloatEngine::new(Some(1));
    let mut data = 1.5_f32.to_le_bytes().to_vec();
    data.extend_from_slice(&TAIL);

    // 分两次喂入
    let frames1 = engine.feed(&data[..3]).frames;
    assert!(frames1.is_empty());
    let frames2 = engine.feed(&data[3..]).frames;
    assert_eq!(frames2.len(), 1);
    assert_eq!(frames2[0].channels, vec![1.5]);
}

#[test]
fn test_auto_mode_detect_channels() {
    // 自动模式: 由首帧 payload_len / 4 推断
    let mut engine = JustFloatEngine::new(None);
    assert!(engine.is_auto_mode());
    assert_eq!(engine.detected_channels(), None);

    let mut data = Vec::new();
    data.extend_from_slice(&10.0_f32.to_le_bytes());
    data.extend_from_slice(&20.0_f32.to_le_bytes());
    data.extend_from_slice(&30.0_f32.to_le_bytes());
    data.extend_from_slice(&TAIL);

    let frames = engine.feed(&data).frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![10.0, 20.0, 30.0]);
    // 检测到 3 通道
    assert_eq!(engine.detected_channels(), Some(3));
}

#[test]
fn test_manual_mode_not_auto() {
    let engine = JustFloatEngine::new(Some(4));
    assert!(!engine.is_auto_mode());
    assert_eq!(engine.detected_channels(), None);
}

#[test]
fn test_auto_mode_multi_frames() {
    // 自动模式多帧
    let mut engine = JustFloatEngine::new(None);
    let mut data = Vec::new();
    // 第一帧 2 通道
    data.extend_from_slice(&1.0_f32.to_le_bytes());
    data.extend_from_slice(&2.0_f32.to_le_bytes());
    data.extend_from_slice(&TAIL);
    // 第二帧 2 通道
    data.extend_from_slice(&3.0_f32.to_le_bytes());
    data.extend_from_slice(&4.0_f32.to_le_bytes());
    data.extend_from_slice(&TAIL);

    let frames = engine.feed(&data).frames;
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].channels, vec![1.0, 2.0]);
    assert_eq!(frames[1].channels, vec![3.0, 4.0]);
    assert_eq!(engine.detected_channels(), Some(2));
}

#[test]
#[allow(clippy::float_cmp)] // f32 经 le_bytes 往返为位精确字面量
fn test_encode_uses_detected_channels() {
    // 自动模式: 检测后编码使用检测到的通道数
    let mut engine = JustFloatEngine::new(None);
    let mut data = Vec::new();
    data.extend_from_slice(&1.0_f32.to_le_bytes());
    data.extend_from_slice(&2.0_f32.to_le_bytes());
    data.extend_from_slice(&TAIL);
    let _ = engine.feed(&data);
    assert_eq!(engine.detected_channels(), Some(2));

    // 编码单通道 0 = 5.0, 应生成 2 通道帧 (5.0, 0.0) + TAIL
    let encoded = engine.encode_channel(0, 5.0);
    assert_eq!(encoded.len(), 2 * 4 + TAIL.len());
    assert_eq!(&encoded[8..], &TAIL);
    assert_eq!(f32::from_le_bytes(encoded[0..4].try_into().unwrap()), 5.0);
    assert_eq!(f32::from_le_bytes(encoded[4..8].try_into().unwrap()), 0.0);
}

#[test]
#[allow(clippy::float_cmp)] // f32 经 le_bytes 往返为位精确字面量
fn test_encode_channels_roundtrip() {
    let mut engine = JustFloatEngine::new(Some(3));
    let encoded = engine.encode_channels(&[1.5, 2.5, 3.5]);
    assert_eq!(encoded.len(), 3 * 4 + TAIL.len());

    let decoded = engine.feed(&encoded).frames;
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].channels, vec![1.5, 2.5, 3.5]);
}

#[test]
fn test_encode_frame_auto_init() {
    // 自动模式 + encode_frame (尚未 feed): 应按帧通道数推断
    let mut engine = JustFloatEngine::new(None);
    let frame = DataFrame::new(vec![1.0, 2.0, 3.0, 4.0]);
    let encoded = engine.encode_frame(&frame);
    assert_eq!(encoded.len(), 4 * 4 + TAIL.len());
    assert_eq!(engine.detected_channels(), Some(4));
}

#[test]
fn test_split_aligned_equivalence() {
    // 顺序/并行等价性: 多帧 + 垃圾前缀 + 半帧尾
    let mut data = Vec::new();
    // 垃圾前缀 (使首个 TAIL 前的 payload 长度非 4 倍数, 被跳过)
    data.extend_from_slice(&[0x11, 0x22]);
    // 帧 1: 因垃圾前缀被判定无效 (顺序/并行行为一致)
    data.extend_from_slice(&1.0_f32.to_le_bytes());
    data.extend_from_slice(&2.0_f32.to_le_bytes());
    data.extend_from_slice(&TAIL);
    // 帧 2: 2 通道, 有效
    data.extend_from_slice(&3.0_f32.to_le_bytes());
    data.extend_from_slice(&4.0_f32.to_le_bytes());
    data.extend_from_slice(&TAIL);
    // 半帧尾
    data.extend_from_slice(&5.0_f32.to_le_bytes());

    // 顺序解析全量
    let mut seq_engine = JustFloatEngine::new(Some(2));
    let seq_frames = seq_engine.feed(&data).frames;

    // 并行: split_aligned 切分 + 逐块 new_worker().feed + append 合并
    let ranges = seq_engine
        .split_aligned(&data, 3)
        .expect("justfloat 应支持并行切分");
    let tail_start = ranges.last().map_or(0, |r| r.end);
    let mut merged = FeedOutput::default();
    let mut concat = Vec::new();
    for r in &ranges {
        let mut w = seq_engine.new_worker();
        merged.append(w.feed(&data[r.clone()]));
        concat.extend_from_slice(&data[r.clone()]);
    }
    concat.extend_from_slice(&data[tail_start..]);

    // concat(块) + tail == 原 data; 半帧尾留在 tail 中
    assert_eq!(concat, data);
    assert_eq!(&data[tail_start..], &5.0_f32.to_le_bytes());

    // 结果逐项相等 (忽略 timestamp): 仅帧 2 有效
    let norm = |f: &DataFrame| f.channels.clone();
    let seq_norm: Vec<_> = seq_frames.iter().map(norm).collect();
    let par_norm: Vec<_> = merged.frames.iter().map(norm).collect();
    assert_eq!(seq_norm, par_norm);
    assert_eq!(seq_frames.len(), 1);
    assert_eq!(seq_frames[0].channels, vec![3.0, 4.0]);
}

#[test]
fn test_payload_not_multiple_of_4_skipped() {
    // payload 长度非 4 倍数 → 该帧被跳过, 不消耗后续正确帧
    let mut engine = JustFloatEngine::new(Some(2));
    let mut data = Vec::new();
    // 假帧: 仅 1 字节 (非 4 倍数), 后面跟 TAIL
    data.push(0x42);
    data.extend_from_slice(&TAIL);
    // 真帧
    data.extend_from_slice(&5.0_f32.to_le_bytes());
    data.extend_from_slice(&6.0_f32.to_le_bytes());
    data.extend_from_slice(&TAIL);

    let frames = engine.feed(&data).frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![5.0, 6.0]);
}

#[test]
fn test_empty_tail_returns_no_frames() {
    let mut engine = JustFloatEngine::new(Some(2));
    let frames = engine.feed(&[]).frames;
    assert!(frames.is_empty());
}

#[test]
fn test_buffer_overflow_truncation() {
    // 喂入无 TAIL 的超长数据, 触发表护截断 (8192 上限, 保留 4096)
    let mut engine = JustFloatEngine::new(Some(2));
    let big = vec![0u8; 10_000];
    let frames = engine.feed(&big).frames;
    assert!(frames.is_empty());
}
