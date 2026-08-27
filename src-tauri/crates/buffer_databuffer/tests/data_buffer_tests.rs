use buffer_databuffer::DataBuffer;
use vofa_core::DataFrame;

#[test]
fn push_and_get_recent() {
    let mut buf = DataBuffer::new(100, 2);
    buf.push_frame(&DataFrame::new(vec![1.0, 2.0]));
    buf.push_frame(&DataFrame::new(vec![3.0, 4.0]));
    buf.push_frame(&DataFrame::new(vec![5.0, 6.0]));

    let w = buf.get_recent(2);
    assert_eq!(w.channel_count, 2);
    assert_eq!(w.channels[0], vec![3.0, 5.0]);
    assert_eq!(w.channels[1], vec![4.0, 6.0]);
}

#[test]
fn empty() {
    let buf = DataBuffer::new(100, 4);
    let w = buf.get_recent(10);
    assert_eq!(w.channel_count, 4);
    assert!(w.timestamps.is_empty());
    assert!(w.channels.is_empty() || w.channels.iter().all(|c| c.is_empty()));
}

#[test]
fn clear() {
    let mut buf = DataBuffer::new(100, 2);
    buf.push_frame(&DataFrame::new(vec![1.0, 2.0]));
    buf.clear();
    assert_eq!(buf.point_count(), 0);
}

#[test]
fn auto_expand_channels() {
    let mut buf = DataBuffer::new(100, 2);
    buf.push_frame(&DataFrame::new(vec![1.0, 2.0]));
    assert_eq!(buf.channel_count(), 2);
    buf.push_frame(&DataFrame::new(vec![1.0, 2.0, 3.0, 4.0]));
    assert_eq!(buf.channel_count(), 4);
    let w = buf.get_recent(2);
    assert_eq!(w.channels[0], vec![1.0, 1.0]);
    assert_eq!(w.channels[3], vec![4.0]);
}

#[test]
fn set_channels() {
    let mut buf = DataBuffer::new(100, 2);
    buf.push_frame(&DataFrame::new(vec![1.0, 2.0]));
    buf.set_channels(4);
    assert_eq!(buf.channel_count(), 4);
    assert_eq!(buf.point_count(), 0);
}

#[test]
fn get_channel() {
    let mut buf = DataBuffer::new(100, 3);
    buf.push_frame(&DataFrame::new(vec![10.0, 20.0, 30.0]));
    buf.push_frame(&DataFrame::new(vec![11.0, 21.0, 31.0]));
    assert_eq!(buf.get_channel(0, 2), vec![10.0, 11.0]);
    assert_eq!(buf.get_channel(2, 2), vec![30.0, 31.0]);
    assert_eq!(buf.get_channel(99, 2), Vec::<f32>::new());
}

#[test]
fn zero_channels_clamps_to_one() {
    let buf = DataBuffer::new(100, 0);
    assert_eq!(buf.channel_count(), 1);
}

#[test]
fn zero_max_points_clamps_to_one() {
    let mut buf = DataBuffer::new(0, 2);
    buf.push_frame(&DataFrame::new(vec![1.0, 2.0]));
    assert_eq!(buf.point_count(), 1);
}

#[test]
fn version_monotonic() {
    let mut buf = DataBuffer::new(100, 2);
    let v0 = buf.version();
    buf.push_frame(&DataFrame::new(vec![1.0, 2.0]));
    let v1 = buf.version();
    buf.push_frame(&DataFrame::new(vec![3.0, 4.0]));
    let v2 = buf.version();
    assert!(v1 > v0);
    assert!(v2 > v1);
}

#[test]
fn set_max_points_smaller_drops_old() {
    let mut buf = DataBuffer::new(100, 1);
    for i in 0..10 {
        buf.push_frame(&DataFrame::new(vec![i as f32]));
    }
    assert_eq!(buf.point_count(), 10);
    buf.set_max_points(3);
    assert_eq!(buf.max_points(), 3);
    assert_eq!(buf.point_count(), 3);
    let w = buf.get_recent(3);
    assert_eq!(w.channels[0], vec![7.0, 8.0, 9.0]);
}

#[test]
fn set_max_points_larger_keeps_all() {
    let mut buf = DataBuffer::new(3, 1);
    for i in 0..3 {
        buf.push_frame(&DataFrame::new(vec![i as f32]));
    }
    buf.set_max_points(10);
    assert_eq!(buf.point_count(), 3);
    assert_eq!(buf.max_points(), 10);
}

#[test]
fn clear_resets_version_only_via_push() {
    let mut buf = DataBuffer::new(100, 2);
    buf.push_frame(&DataFrame::new(vec![1.0, 2.0]));
    let v_before = buf.version();
    buf.clear();
    assert_eq!(buf.version(), v_before);
}

#[test]
fn push_frame_with_more_channels_pads_zeros() {
    let mut buf = DataBuffer::new(100, 4);
    buf.push_frame(&DataFrame::new(vec![10.0, 20.0]));
    let w = buf.get_recent(1);
    assert_eq!(w.channels[0], vec![10.0]);
    assert_eq!(w.channels[1], vec![20.0]);
    assert_eq!(w.channels[2], vec![0.0]);
    assert_eq!(w.channels[3], vec![0.0]);
}

#[test]
fn frame_with_extra_channels_ignored() {
    let mut buf = DataBuffer::new(100, 2);
    buf.push_frame(&DataFrame::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]));
    assert_eq!(buf.channel_count(), 5);
}
