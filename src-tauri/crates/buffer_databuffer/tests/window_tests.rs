use buffer_databuffer::{DataBuffer, WaveformWindow};
use vofa_core::DataFrame;

#[test]
fn get_window_with_derived() {
    let mut buf = DataBuffer::new(100, 1);
    for i in 0..5 {
        buf.push_frame(&DataFrame::new(vec![i as f32]));
        buf.push_derived("wave1", "math1", (i * 10) as f32);
    }
    let w = buf.get_recent(5);
    assert_eq!(w.channels[0], vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    let derived = w.derived.get("wave1").unwrap().get("math1").unwrap();
    assert_eq!(derived, &vec![0.0, 10.0, 20.0, 30.0, 40.0]);
}

#[test]
fn get_recent_empty_buffer() {
    let buf = DataBuffer::new(100, 4);
    let w = buf.get_recent(10);
    assert!(w.timestamps.is_empty());
    assert_eq!(w.channel_count, 4);
    assert_eq!(w.buffer_points, 0);
    assert_eq!(w.buffer_capacity, 100);
}

#[test]
fn get_window_empty_buffer() {
    let buf = DataBuffer::new(100, 2);
    let w = buf.get_window(-1000, 0);
    assert!(w.timestamps.is_empty());
    assert_eq!(w.channel_count, 2);
}

#[test]
fn get_recent_derived_skips_empty_entries() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame(&DataFrame::new(vec![1.0]));
    let _idx = buf.derived_index_of("wave1", "math1");
    let w = buf.get_recent(1);
    assert!(w.derived.is_empty());
}

#[test]
fn get_window_negative_range_clamps_to_zero() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame(&DataFrame::new(vec![1.0]));
    let w = buf.get_window(-1_000_000, 0);
    assert!(!w.timestamps.is_empty());
}

#[test]
fn waveform_window_serde_round_trip() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame(&DataFrame::new(vec![1.0]));
    buf.push_derived("wave1", "math1", 10.0);
    let w = buf.get_recent(1);
    let json = serde_json::to_string(&w).unwrap();
    let back: WaveformWindow = serde_json::from_str(&json).unwrap();
    assert_eq!(back.channel_count, w.channel_count);
    assert_eq!(back.buffer_capacity, w.buffer_capacity);
    assert_eq!(back.channels.len(), w.channels.len());
}

#[test]
fn get_recent_count_exceeds_buffer_returns_all() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame(&DataFrame::new(vec![1.0]));
    buf.push_frame(&DataFrame::new(vec![2.0]));
    let w = buf.get_recent(10);
    assert_eq!(w.channels[0].len(), 2);
}
