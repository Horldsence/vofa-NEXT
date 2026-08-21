use buffer_databuffer::DataBuffer;
use vofa_core::DataFrame;

#[test]
fn push_derived_aligned_with_timestamps() {
    let mut buf = DataBuffer::new(100, 2);
    buf.push_frame(&DataFrame::new(vec![1.0, 2.0]));
    buf.push_derived("wave1", "math1", 10.0);
    buf.push_frame(&DataFrame::new(vec![3.0, 4.0]));
    buf.push_derived("wave1", "math1", 30.0);
    buf.push_frame(&DataFrame::new(vec![5.0, 6.0]));
    buf.push_derived("wave1", "math1", 50.0);

    let w = buf.get_recent(3);
    assert_eq!(w.channels[0], vec![1.0, 3.0, 5.0]);
    let derived = w.derived.get("wave1").unwrap().get("math1").unwrap();
    assert_eq!(derived, &vec![10.0, 30.0, 50.0]);
}

#[test]
fn derived_created_later_pads_nan() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame(&DataFrame::new(vec![1.0]));
    buf.push_frame(&DataFrame::new(vec![2.0]));
    buf.push_frame(&DataFrame::new(vec![3.0]));
    buf.push_derived("wave1", "math1", 30.0);
    buf.push_frame(&DataFrame::new(vec![4.0]));
    buf.push_derived("wave1", "math1", 40.0);

    let w = buf.get_recent(4);
    assert_eq!(w.channels[0], vec![1.0, 2.0, 3.0, 4.0]);
    let derived = w.derived.get("wave1").unwrap().get("math1").unwrap();
    assert_eq!(derived.len(), 4);
    assert!(derived[0].is_nan());
    assert!(derived[1].is_nan());
    assert_eq!(derived[2], 30.0);
    assert_eq!(derived[3], 40.0);
}

#[test]
fn multiple_derived_sources() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame(&DataFrame::new(vec![1.0]));
    buf.push_derived("wave1", "math1", 10.0);
    buf.push_derived("wave1", "math2", 20.0);
    buf.push_frame(&DataFrame::new(vec![2.0]));
    buf.push_derived("wave1", "math1", 30.0);
    buf.push_derived("wave1", "math2", 40.0);

    let w = buf.get_recent(2);
    let sink_derived = w.derived.get("wave1").unwrap();
    assert_eq!(sink_derived.get("math1").unwrap(), &vec![10.0, 30.0]);
    assert_eq!(sink_derived.get("math2").unwrap(), &vec![20.0, 40.0]);
}

#[test]
fn multiple_derived_sinks() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame(&DataFrame::new(vec![1.0]));
    buf.push_derived("wave1", "math1", 10.0);
    buf.push_derived("wave2", "math2", 20.0);

    let w = buf.get_recent(1);
    assert_eq!(
        w.derived.get("wave1").unwrap().get("math1").unwrap(),
        &vec![10.0]
    );
    assert_eq!(
        w.derived.get("wave2").unwrap().get("math2").unwrap(),
        &vec![20.0]
    );
}

#[test]
fn clear_derived() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame(&DataFrame::new(vec![1.0]));
    buf.push_derived("wave1", "math1", 10.0);
    assert!(!buf.get_recent(1).derived.is_empty());

    buf.clear_derived();
    let w = buf.get_recent(1);
    assert!(w.derived.is_empty());
    assert_eq!(w.channels[0], vec![1.0]);
}

#[test]
fn remove_derived_sink() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame(&DataFrame::new(vec![1.0]));
    buf.push_derived("wave1", "math1", 10.0);
    buf.push_derived("wave2", "math2", 20.0);

    buf.remove_derived_sink("wave1");
    let w = buf.get_recent(1);
    assert!(!w.derived.contains_key("wave1"));
    assert!(w.derived.contains_key("wave2"));
}

#[test]
fn derived_ringbuffer_overflow() {
    let mut buf = DataBuffer::new(3, 1);
    for i in 0..5 {
        buf.push_frame(&DataFrame::new(vec![i as f32]));
        buf.push_derived("wave1", "math1", (i * 10) as f32);
    }
    let w = buf.get_recent(3);
    assert_eq!(w.channels[0], vec![2.0, 3.0, 4.0]);
    let derived = w.derived.get("wave1").unwrap().get("math1").unwrap();
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
    let mut buf = DataBuffer::new(100, 1);
    let i1 = buf.derived_index_of("wave1", "math1");
    let i2 = buf.derived_index_of("wave1", "math1");
    assert_eq!(i1, i2);
}

#[test]
fn push_derived_idx_out_of_bounds_silently_drops() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame(&DataFrame::new(vec![1.0]));
    buf.push_derived_idx(999, 42.0);
    let w = buf.get_recent(1);
    assert!(w.derived.is_empty());
}

#[test]
fn remove_derived_sink_rebuilds_index() {
    let mut buf = DataBuffer::new(100, 1);
    let _i_a = buf.derived_index_of("waveA", "math1");
    let _i_b = buf.derived_index_of("waveB", "math1");
    buf.remove_derived_sink("waveA");
    let new_i_b = buf.derived_index_of("waveB", "math1");
    assert_eq!(new_i_b, 0);
    buf.push_derived_idx(new_i_b, 99.0);
    let w = buf.get_recent(1);
    assert!(!w.derived.contains_key("waveA"));
    assert_eq!(w.derived.get("waveB").unwrap().get("math1").unwrap(), &vec![99.0]);
}