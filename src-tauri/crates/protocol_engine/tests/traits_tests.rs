//! 集成测试: `FeedOutput` / `ParsedInput` / `ProtocolEngine` 默认实现

use can_types::CanDirection;
use protocol_engine::{FeedOutput, InputFormat, ParsedInput, ProtocolEngine};
use vofa_core::DataFrame;

/// Mock 引擎 — 始终返回单一 DataFrame 用于测试默认实现
struct MockEngine {
    label: &'static str,
    auto_mode: bool,
    detected: Option<usize>,
}

impl MockEngine {
    fn new(label: &'static str) -> Self {
        Self {
            label,
            auto_mode: false,
            detected: None,
        }
    }
}

impl ProtocolEngine for MockEngine {
    fn feed(&mut self, data: &[u8]) -> FeedOutput {
        // 按字节数生成等长通道值的 DataFrame
        let channels: Vec<f32> = data.iter().map(|&b| b as f32).collect();
        FeedOutput::from_frames(vec![DataFrame::new(channels)])
    }
    fn encode_channel(&mut self, _channel: usize, value: f32) -> Vec<u8> {
        vec![value as u8]
    }
    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8> {
        values.iter().map(|&v| v as u8).collect()
    }
    fn name(&self) -> &str {
        self.label
    }
    fn detected_channels(&self) -> Option<usize> {
        self.detected
    }
    fn is_auto_mode(&self) -> bool {
        self.auto_mode
    }
    fn new_worker(&self) -> Box<dyn ProtocolEngine> {
        Box::new(MockEngine::new(self.label))
    }
}

/// Mock 引擎 — 始终返回 CAN 帧
struct MockCanEngine;

impl ProtocolEngine for MockCanEngine {
    fn feed(&mut self, _data: &[u8]) -> FeedOutput {
        use can_types::CanFrame;
        FeedOutput::from_can_frames(vec![CanFrame {
            timestamp: 0,
            id: 0x123,
            extended: false,
            rtr: false,
            dlc: 0,
            data: Vec::new(),
            direction: CanDirection::Rx,
        }])
    }
    fn encode_channel(&mut self, _channel: usize, _value: f32) -> Vec<u8> {
        Vec::new()
    }
    fn encode_channels(&mut self, _values: &[f32]) -> Vec<u8> {
        Vec::new()
    }
    fn name(&self) -> &str {
        "MockCan"
    }
    fn new_worker(&self) -> Box<dyn ProtocolEngine> {
        Box::new(MockCanEngine)
    }
}

/// Mock 引擎 — feed 不产生任何结构化输出 (用于 RawBytes 回退路径)
struct EmptyEngine;

impl ProtocolEngine for EmptyEngine {
    fn feed(&mut self, _data: &[u8]) -> FeedOutput {
        FeedOutput::default()
    }
    fn encode_channel(&mut self, _channel: usize, _value: f32) -> Vec<u8> {
        Vec::new()
    }
    fn encode_channels(&mut self, _values: &[f32]) -> Vec<u8> {
        Vec::new()
    }
    fn name(&self) -> &str {
        "Empty"
    }
    fn new_worker(&self) -> Box<dyn ProtocolEngine> {
        Box::new(EmptyEngine)
    }
}

// ===== FeedOutput =====

#[test]
fn feed_output_default_is_empty() {
    let out = FeedOutput::default();
    assert!(out.frames.is_empty());
    assert!(out.can_frames.is_empty());
    assert!(out.logic_samples.is_empty());
    assert!(out.decoded_events.is_empty());
}

#[test]
fn feed_output_from_frames_only_fills_frames() {
    let df = DataFrame::new(vec![1.0, 2.0]);
    let out = FeedOutput::from_frames(vec![df]);
    assert_eq!(out.frames.len(), 1);
    assert!(out.can_frames.is_empty());
}

#[test]
fn feed_output_from_can_frames_only_fills_can() {
    let cf = can_types::CanFrame {
        timestamp: 0,
        id: 1,
        extended: false,
        rtr: false,
        dlc: 0,
        data: Vec::new(),
        direction: CanDirection::Rx,
    };
    let out = FeedOutput::from_can_frames(vec![cf]);
    assert!(out.frames.is_empty());
    assert_eq!(out.can_frames.len(), 1);
}

#[test]
fn feed_output_append_merges_all_fields() {
    let mut a = FeedOutput::default();
    a.frames.push(DataFrame::new(vec![1.0]));
    a.can_frames.push(can_types::CanFrame {
        timestamp: 0,
        id: 1,
        extended: false,
        rtr: false,
        dlc: 0,
        data: Vec::new(),
        direction: CanDirection::Rx,
    });

    let mut b = FeedOutput::default();
    b.frames.push(DataFrame::new(vec![2.0]));

    a.append(b);
    assert_eq!(a.frames.len(), 2);
    assert_eq!(a.can_frames.len(), 1);
}

// ===== ParsedInput =====

#[test]
fn parsed_input_error_constructor() {
    let p = ParsedInput::error("boom");
    match p {
        ParsedInput::Error { message } => assert_eq!(message, "boom"),
        other => panic!("expected Error, got {:?}", other),
    }
}

// ===== parse_input 默认实现 =====

#[test]
fn parse_input_default_with_hex_format() {
    let mut engine = MockEngine::new("Mock");
    let result = engine.parse_input("AA 01", InputFormat::Hex);
    match result {
        ParsedInput::Frames(frames) => {
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0].channels, vec![0xAA as f32, 0x01 as f32]);
        }
        other => panic!("expected Frames, got {:?}", other),
    }
}

#[test]
fn parse_input_default_with_auto_resolves_hex() {
    let mut engine = MockEngine::new("Mock");
    let result = engine.parse_input("AA 01", InputFormat::Auto);
    match result {
        ParsedInput::Frames(frames) => {
            assert_eq!(frames.len(), 1);
        }
        other => panic!("expected Frames, got {:?}", other),
    }
}

#[test]
fn parse_input_default_with_ascii_format() {
    let mut engine = MockEngine::new("Mock");
    let result = engine.parse_input("Hi", InputFormat::Ascii);
    match result {
        ParsedInput::Frames(frames) => {
            assert_eq!(frames[0].channels, vec![b'H' as f32, b'i' as f32]);
        }
        other => panic!("expected Frames, got {:?}", other),
    }
}

#[test]
fn parse_input_default_collects_can_frames() {
    let mut engine = MockCanEngine;
    let result = engine.parse_input("AA 01 02 BB", InputFormat::Hex);
    match result {
        ParsedInput::CanFrames(frames) => {
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0].id, 0x123);
        }
        other => panic!("expected CanFrames, got {:?}", other),
    }
}

#[test]
fn parse_input_default_falls_back_to_raw_bytes() {
    let mut engine = EmptyEngine;
    let result = engine.parse_input("AA 01 02 BB", InputFormat::Hex);
    match result {
        ParsedInput::RawBytes(bytes) => {
            assert_eq!(bytes, vec![0xAA, 0x01, 0x02, 0xBB]);
        }
        other => panic!("expected RawBytes, got {:?}", other),
    }
}

#[test]
fn parse_input_default_empty_input_returns_error() {
    let mut engine = MockEngine::new("Mock");
    let result = engine.parse_input("", InputFormat::Hex);
    match result {
        ParsedInput::Error { message } => assert!(message.contains("空")),
        other => panic!("expected Error, got {:?}", other),
    }
}

#[test]
fn parse_input_default_invalid_hex_returns_error() {
    let mut engine = MockEngine::new("Mock");
    let result = engine.parse_input("ZZ", InputFormat::Hex);
    match result {
        ParsedInput::Error { .. } => {}
        other => panic!("expected Error, got {:?}", other),
    }
}

// ===== 默认 trait 方法 =====

#[test]
fn encode_frame_default_uses_encode_channels() {
    let mut engine = MockEngine::new("Mock");
    let df = DataFrame::new(vec![10.0, 20.0]);
    let bytes = engine.encode_frame(&df);
    assert_eq!(bytes, vec![10, 20]);
}

#[test]
fn default_trait_methods_return_baseline() {
    let mut engine = MockEngine::new("Mock");
    assert_eq!(engine.detected_channels(), None);
    assert!(!engine.is_auto_mode());
    assert_eq!(engine.encode_can(&can_types::CanFrame {
        timestamp: 0,
        id: 0,
        extended: false,
        rtr: false,
        dlc: 0,
        data: Vec::new(),
        direction: CanDirection::Rx,
    }), Vec::<u8>::new());
    assert!(engine.split_aligned(&[1, 2, 3], 2).is_none());
    assert_eq!(engine.take_pending(), Vec::<u8>::new());
}