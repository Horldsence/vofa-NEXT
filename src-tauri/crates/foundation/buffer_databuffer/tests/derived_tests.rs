#![allow(clippy::float_cmp, clippy::cast_precision_loss)] // 测试数值断言: 精确比较 + 小整数转 f32 无精度问题
use buffer_databuffer::{DataBuffer, WaveformWindow};

#[test]
fn detached_writer_updates_live_version_but_not_frozen_clone() {
    let mut buffer = DataBuffer::new(32, 1);
    buffer.push_frame_at(1_000, &[1.0]);
    let writer = buffer.derived_writer();
    let index = writer.port_index_of("wave", "math", "result");
    let frozen = buffer.clone();
    let before = buffer.version();
    writer.append([(index, 1_000, 9.0)]);
    assert!(buffer.version() > before);
    assert_eq!(buffer.get_derived(index, 1), vec![9.0]);
    assert!(frozen.get_derived(index, 1).is_empty());
    assert_eq!(frozen.version(), before);
}

#[test]
fn stale_writer_cannot_write_into_reused_indices_after_clear() {
    let buffer = DataBuffer::new(32, 1);
    let old_writer = buffer.derived_writer();
    let old_index = old_writer.port_index_of("old", "math", "result");
    buffer.clear_derived();
    let new_index = buffer.derived_port_index_of("new", "math", "result");
    assert_eq!(old_index, new_index);
    old_writer.append([(old_index, 1_000, 99.0)]);
    assert!(buffer.get_derived(new_index, 10).is_empty());
    assert_eq!(
        old_writer.port_index_of("old", "math", "result"),
        usize::MAX
    );
}

fn derived_values<'a>(window: &'a WaveformWindow, sink: &str, source: &str) -> &'a Vec<f32> {
    &window.derived[sink][source][""]
}

#[test]
fn recent_derived_query_preserves_duplicates_and_gaps_after_wrap() {
    let mut buffer = DataBuffer::new(1_000, 1);
    let writer = buffer.derived_writer();
    let idx = writer.port_index_of("wave", "math", "");
    for i in 0..3_000_u64 {
        let timestamp = i / 2;
        buffer.push_frame_at(timestamp, &[i as f32]);
        if !(2_990..2_994).contains(&i) {
            writer.push(idx, timestamp, i as f32);
        }
    }
    let window = buffer.get_recent(12);
    let values = derived_values(&window, "wave", "math");
    assert_eq!(&values[..2], &[2_988.0, 2_989.0]);
    assert!(values[2..6].iter().all(|v| v.is_nan()));
    assert_eq!(
        &values[6..],
        &[2_994.0, 2_995.0, 2_996.0, 2_997.0, 2_998.0, 2_999.0]
    );
}

#[test]
fn push_derived_aligned_by_timestamp() {
    let mut buf = DataBuffer::new(100, 2);
    // 派生值携带与原始帧相同的显式时间戳 → 窗口按时间精确对齐
    buf.push_frame_at(1_000, &[1.0, 2.0]);
    buf.push_derived_ts_idx(buf.derived_index_of("wave1", "math1"), 1_000, 10.0);
    buf.push_frame_at(2_000, &[3.0, 4.0]);
    buf.push_derived_ts_idx(buf.derived_index_of("wave1", "math1"), 2_000, 30.0);
    buf.push_frame_at(3_000, &[5.0, 6.0]);
    buf.push_derived_ts_idx(buf.derived_index_of("wave1", "math1"), 3_000, 50.0);

    let w = buf.get_recent(3);
    assert_eq!(w.channels[0], vec![1.0, 3.0, 5.0]);
    let derived = derived_values(&w, "wave1", "math1");
    assert_eq!(derived, &vec![10.0, 30.0, 50.0]);
}

#[test]
fn derived_created_later_pads_nan() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame_at(1_000, &[1.0]);
    buf.push_frame_at(2_000, &[2.0]);
    // 前两个时间戳无派生值 (求值尚未覆盖) → NaN 对齐
    let math = buf.derived_index_of("wave1", "math1");
    buf.push_derived_ts_idx(math, 3_000, 30.0);
    buf.push_frame_at(3_000, &[3.0]);
    buf.push_derived_ts_idx(math, 4_000, 40.0);
    buf.push_frame_at(4_000, &[4.0]);

    let w = buf.get_recent(4);
    assert_eq!(w.channels[0], vec![1.0, 2.0, 3.0, 4.0]);
    let derived = derived_values(&w, "wave1", "math1");
    assert_eq!(derived.len(), 4);
    assert!(derived[0].is_nan());
    assert!(derived[1].is_nan());
    assert_eq!(derived[2], 30.0);
    assert_eq!(derived[3], 40.0);
}

/// 求值落后于记录: 缺失时间戳的位置补 NaN, 已求值部分不错位
#[test]
fn eval_lag_shows_gap_not_misalignment() {
    let mut buf = DataBuffer::new(100, 1);
    let math = buf.derived_index_of("wave1", "math1");
    // 记录平面全速入库
    for i in 1..=4_u64 {
        buf.push_frame_at(i * 1_000, &[i as f32]);
    }
    // 求值平面只完成 2/4 (第二批被丢弃)
    buf.push_derived_ts_idx(math, 1_000, 10.0);
    buf.push_derived_ts_idx(math, 3_000, 30.0);

    let w = buf.get_recent(4);
    let derived = derived_values(&w, "wave1", "math1");
    assert_eq!(derived[0], 10.0);
    assert!(derived[1].is_nan(), "丢批处应显示缺口");
    assert_eq!(derived[2], 30.0);
    assert!(derived[3].is_nan());
    // 原始通道不受影响
    assert_eq!(w.channels[0], vec![1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn multiple_derived_sources() {
    let mut buf = DataBuffer::new(100, 1);
    let m1 = buf.derived_index_of("wave1", "math1");
    let m2 = buf.derived_index_of("wave1", "math2");
    buf.push_frame_at(1_000, &[1.0]);
    buf.push_derived_ts_idx(m1, 1_000, 10.0);
    buf.push_derived_ts_idx(m2, 1_000, 20.0);
    buf.push_frame_at(2_000, &[2.0]);
    buf.push_derived_ts_idx(m1, 2_000, 30.0);
    buf.push_derived_ts_idx(m2, 2_000, 40.0);

    let w = buf.get_recent(2);
    assert_eq!(derived_values(&w, "wave1", "math1"), &vec![10.0, 30.0]);
    assert_eq!(derived_values(&w, "wave1", "math2"), &vec![20.0, 40.0]);
}

#[test]
fn multiple_outputs_from_one_source_remain_distinct_and_aligned() {
    let mut buf = DataBuffer::new(100, 1);
    let out_a = buf.derived_port_index_of("wave", "custom", "out-a");
    let out_b = buf.derived_port_index_of("wave", "custom", "out-b");
    for index in 0..3 {
        let ts = u64::try_from(index + 1).unwrap_or(0) * 1_000;
        buf.push_frame_at(ts, &[index as f32]);
        buf.push_derived_ts_idx(out_a, ts, 10.0 + index as f32);
        buf.push_derived_ts_idx(out_b, ts, 20.0 + index as f32);
    }

    let window = buf.get_recent(3);
    assert_eq!(
        window.derived["wave"]["custom"]["out-a"],
        vec![10.0, 11.0, 12.0]
    );
    assert_eq!(
        window.derived["wave"]["custom"]["out-b"],
        vec![20.0, 21.0, 22.0]
    );
    assert!(window.derived["wave"]["custom"]
        .values()
        .all(|values| values.len() == window.timestamps.len()));
}

#[test]
fn multiple_derived_sinks() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame_at(1_000, &[1.0]);
    buf.push_derived_ts_idx(buf.derived_index_of("wave1", "math1"), 1_000, 10.0);
    buf.push_derived_ts_idx(buf.derived_index_of("wave2", "math2"), 1_000, 20.0);

    let w = buf.get_recent(1);
    assert_eq!(derived_values(&w, "wave1", "math1"), &vec![10.0]);
    assert_eq!(derived_values(&w, "wave2", "math2"), &vec![20.0]);
}

#[test]
fn clear_derived() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame_at(1_000, &[1.0]);
    buf.push_derived_ts_idx(buf.derived_index_of("wave1", "math1"), 1_000, 10.0);
    assert!(!buf.get_recent(1).derived.is_empty());

    buf.clear_derived();
    let w = buf.get_recent(1);
    assert!(w.derived.is_empty());
    assert_eq!(w.channels[0], vec![1.0]);
}

#[test]
fn remove_derived_sink() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame_at(1_000, &[1.0]);
    buf.push_derived_ts_idx(buf.derived_index_of("wave1", "math1"), 1_000, 10.0);
    buf.push_derived_ts_idx(buf.derived_index_of("wave2", "math2"), 1_000, 20.0);

    buf.remove_derived_sink("wave1");
    let w = buf.get_recent(1);
    assert!(!w.derived.contains_key("wave1"));
    assert!(w.derived.contains_key("wave2"));
}

#[test]
fn derived_ringbuffer_overflow() {
    let mut buf = DataBuffer::new(3, 1);
    let math = buf.derived_index_of("wave1", "math1");
    for i in 0..5 {
        let ts = u64::try_from(i + 1).unwrap_or(0) * 1_000;
        buf.push_frame_at(ts, &[i as f32]);
        buf.push_derived_ts_idx(math, ts, (i * 10) as f32);
    }
    let w = buf.get_recent(3);
    assert_eq!(w.channels[0], vec![2.0, 3.0, 4.0]);
    let derived = derived_values(&w, "wave1", "math1");
    assert_eq!(derived, &vec![20.0, 30.0, 40.0]);
}

#[test]
fn derived_empty_buffer() {
    let buf = DataBuffer::new(100, 2);
    let w = buf.get_recent(10);
    assert!(w.derived.is_empty());
}

#[test]
fn derived_index_of_idempotent() {
    let buf = DataBuffer::new(100, 1);
    let i1 = buf.derived_index_of("wave1", "math1");
    let i2 = buf.derived_index_of("wave1", "math1");
    assert_eq!(i1, i2);
}

#[test]
fn push_derived_idx_out_of_bounds_silently_drops() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame_at(1_000, &[1.0]);
    buf.push_derived_ts_idx(999, 1_000, 42.0);
    let w = buf.get_recent(1);
    assert!(w.derived.is_empty());
}

#[test]
fn remove_derived_sink_rebuilds_index() {
    let mut buf = DataBuffer::new(100, 1);
    let _i_a = buf.derived_index_of("waveA", "math1");
    buf.derived_index_of("waveB", "math1");
    buf.remove_derived_sink("waveA");
    let new_i_b = buf.derived_index_of("waveB", "math1");
    assert_eq!(new_i_b, 0);
    buf.push_frame_at(1_000, &[1.0]);
    buf.push_derived_ts_idx(new_i_b, 1_000, 99.0);
    let w = buf.get_recent(1);
    assert!(!w.derived.contains_key("waveA"));
    assert_eq!(derived_values(&w, "waveB", "math1"), &vec![99.0]);
}
