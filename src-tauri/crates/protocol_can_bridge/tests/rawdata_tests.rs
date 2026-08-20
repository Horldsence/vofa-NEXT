//! RawData 协议引擎集成测试 — 透传语义

use protocol_can_bridge::RawDataEngine;
use protocol_engine::ProtocolEngine;
use vofa_core::DataFrame;

#[test]
fn test_feed_returns_empty_feed_output() {
    let mut engine = RawDataEngine::new();
    let out = engine.feed(b"\xDE\xAD\xBE\xEF");
    assert!(out.frames.is_empty());
    assert!(out.can_frames.is_empty());
    assert!(out.logic_samples.is_empty());
    assert!(out.decoded_events.is_empty());
}

#[test]
fn test_name_is_rawdata() {
    let engine = RawDataEngine::new();
    assert_eq!(engine.name(), "RawData");
}

#[test]
fn test_encode_channel_produces_ascii_line() {
    let mut engine = RawDataEngine::new();
    let encoded = engine.encode_channel(0, 1.5);
    let s = std::str::from_utf8(&encoded).unwrap();
    assert!(s.ends_with('\n'));
    assert!(s.contains('1'));
}

#[test]
fn test_encode_channels_csv() {
    let mut engine = RawDataEngine::new();
    let encoded = engine.encode_channels(&[1.0, 2.0, 3.0]);
    let s = std::str::from_utf8(&encoded).unwrap();
    assert!(s.contains(','));
}

#[test]
fn test_encode_frame_returns_empty() {
    let mut engine = RawDataEngine::new();
    let frame = DataFrame::new(vec![1.0, 2.0]);
    let bytes = engine.encode_frame(&frame);
    assert!(bytes.is_empty());
}

#[test]
fn test_new_worker_creates_independent_instance() {
    let mut engine = RawDataEngine::new();
    let mut worker = engine.new_worker();
    // worker 独立, feed 不影响原 engine
    let _ = worker.feed(b"junk");
    // 重新调用原 engine 仍正常工作
    let out = engine.feed(b"more");
    assert!(out.frames.is_empty());
}

#[test]
fn test_default_impl_matches_new() {
    let a = RawDataEngine::new();
    let b = RawDataEngine::default();
    assert_eq!(a.name(), b.name());
}