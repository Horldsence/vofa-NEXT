//! 集成测试: FireWater 协议引擎

use protocol_engine::{FeedOutput, ProtocolEngine};
use protocol_float::FireWaterEngine;
use vofa_core::DataFrame;

#[test]
fn test_parse_firewater() {
    let mut engine = FireWaterEngine::new(Some(3));
    let frames = engine.feed(b"1.0,2.0,3.0\n").frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![1.0, 2.0, 3.0]);
}

#[test]
fn test_parse_multiple_lines() {
    let mut engine = FireWaterEngine::new(Some(2));
    let frames = engine.feed(b"1.0,2.0\n3.0,4.0\n").frames;
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].channels, vec![1.0, 2.0]);
    assert_eq!(frames[1].channels, vec![3.0, 4.0]);
}

#[test]
fn test_auto_mode_detect_channels() {
    let mut engine = FireWaterEngine::new(None);
    assert!(engine.is_auto_mode());
    assert_eq!(engine.detected_channels(), None);

    let frames = engine.feed(b"1.0,2.0,3.0,4.0\n").frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(engine.detected_channels(), Some(4));
}

#[test]
fn test_manual_mode_not_auto() {
    let engine = FireWaterEngine::new(Some(4));
    assert!(!engine.is_auto_mode());
    assert_eq!(engine.detected_channels(), None);
}

#[test]
fn test_auto_mode_multi_lines() {
    let mut engine = FireWaterEngine::new(None);
    let frames = engine.feed(b"1.0,2.0\n3.0,4.0\n").frames;
    assert_eq!(frames.len(), 2);
    assert_eq!(engine.detected_channels(), Some(2));
}

#[test]
fn test_partial_line_buffered() {
    let mut engine = FireWaterEngine::new(Some(2));
    let _ = engine.feed(b"1.0,2.");
    let frames = engine.feed(b"0\n").frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![1.0, 2.0]);
}

#[test]
fn test_crlf_line_endings() {
    // \r\n 换行应被识别
    let mut engine = FireWaterEngine::new(Some(2));
    let frames = engine.feed(b"1.0,2.0\r\n3.0,4.0\r\n").frames;
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].channels, vec![1.0, 2.0]);
    assert_eq!(frames[1].channels, vec![3.0, 4.0]);
}

#[test]
fn test_empty_lines_skipped() {
    let mut engine = FireWaterEngine::new(Some(2));
    let frames = engine.feed(b"1.0,2.0\n\n\n3.0,4.0\n").frames;
    assert_eq!(frames.len(), 2);
}

#[test]
fn test_unparseable_token_dropped() {
    // 整行无可解析数字 → 跳过
    let mut engine = FireWaterEngine::new(Some(2));
    let frames = engine.feed(b"abc,def\n1.0,2.0\n").frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![1.0, 2.0]);
}

#[test]
fn test_non_utf8_input_dropped() {
    // 非 UTF-8 字节流: feed 返回空, 不 panic
    let mut engine = FireWaterEngine::new(Some(2));
    let frames = engine.feed(&[0xFF, 0xFE, 0xFD]).frames;
    assert!(frames.is_empty());
}

#[test]
fn test_encode_channels_format() {
    let mut engine = FireWaterEngine::new(Some(3));
    let encoded = engine.encode_channels(&[1.0, 2.5, 3.14]);
    let s = std::str::from_utf8(&encoded).unwrap();
    assert!(s.ends_with('\n'));
    assert!(s.contains(','));
}

#[test]
fn test_encode_frame_auto_init() {
    let mut engine = FireWaterEngine::new(None);
    let frame = DataFrame::new(vec![1.0, 2.0, 3.0, 4.0]);
    let _ = engine.encode_frame(&frame);
    assert_eq!(engine.detected_channels(), Some(4));
}

#[test]
fn test_buffer_overflow_clears() {
    // 超长无 \n 行触发清理 (整 buf 清空)
    let mut engine = FireWaterEngine::new(Some(2));
    let big = vec![b'1'; 10_000];
    let _ = engine.feed(&big);
    // 后续正常 feed 应正常解析
    let frames = engine.feed(b"1.0,2.0\n").frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![1.0, 2.0]);
}

#[test]
fn test_split_aligned_equivalence() {
    // 顺序/并行等价性: 多行 + 跨行尾 (无 \n 的不完整行)
    let data = b"1.0,2.0\n3.0,4.0\n5.0,6";

    // 顺序解析全量
    let mut seq_engine = FireWaterEngine::new(Some(2));
    let seq_frames = seq_engine.feed(data).frames;

    // 并行: split_aligned 切分 + 逐块 new_worker().feed + append 合并
    let ranges = seq_engine
        .split_aligned(data, 2)
        .expect("firewater 应支持并行切分");
    let tail_start = ranges.last().map(|r| r.end).unwrap_or(0);
    let mut merged = FeedOutput::default();
    let mut concat = Vec::new();
    for r in &ranges {
        let mut w = seq_engine.new_worker();
        merged.append(w.feed(&data[r.clone()]));
        concat.extend_from_slice(&data[r.clone()]);
    }
    concat.extend_from_slice(&data[tail_start..]);

    // concat(块) + tail == 原 data; 不完整行留在 tail 中
    assert_eq!(concat, data);
    assert_eq!(&data[tail_start..], b"5.0,6");

    // 结果逐项相等 (忽略 timestamp)
    let norm = |f: &DataFrame| f.channels.clone();
    let seq_norm: Vec<_> = seq_frames.iter().map(norm).collect();
    let par_norm: Vec<_> = merged.frames.iter().map(norm).collect();
    assert_eq!(seq_norm, par_norm);
    assert_eq!(seq_frames.len(), 2);
}