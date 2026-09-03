//! `can_types::can_buffer` 单元测试

use can_types::{CanBuffer, CanDirection, CanFrame};

const fn mk_frame(ts: u64, id: u32) -> CanFrame {
    CanFrame {
        timestamp: ts,
        id,
        extended: false,
        rtr: false,
        dlc: 0,
        data: Vec::new(),
        direction: CanDirection::Rx,
    }
}

#[test]
fn new_buffer_is_empty() {
    let buf = CanBuffer::new(10);
    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
    assert_eq!(buf.version(), 0);
    assert_eq!(buf.capacity(), 10);
}

#[test]
fn push_increments_version() {
    let mut buf = CanBuffer::new(5);
    buf.push(mk_frame(1, 0x100));
    assert_eq!(buf.version(), 1);
    buf.push(mk_frame(2, 0x101));
    assert_eq!(buf.version(), 2);
}

#[test]
fn push_past_capacity_drops_oldest() {
    let mut buf = CanBuffer::new(3);
    buf.push(mk_frame(1, 1));
    buf.push(mk_frame(2, 2));
    buf.push(mk_frame(3, 3));
    buf.push(mk_frame(4, 4));
    assert_eq!(buf.len(), 3);
    let frames = buf.get_recent(10);
    assert_eq!(
        frames.iter().map(|f| f.id).collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
}

#[test]
fn get_recent_returns_in_time_order() {
    let mut buf = CanBuffer::new(10);
    for i in 0..5u32 {
        buf.push(mk_frame(u64::from(i), i));
    }
    let recent = buf.get_recent(3);
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].id, 2);
    assert_eq!(recent[1].id, 3);
    assert_eq!(recent[2].id, 4);
}

#[test]
fn drain_from_advances_cursor() {
    let mut buf = CanBuffer::new(10);
    for i in 0..5u32 {
        buf.push(mk_frame(u64::from(i), i));
    }
    let (items, new_cursor, dropped) = buf.drain_from(0, 3);
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].id, 0);
    assert_eq!(new_cursor, 3);
    assert_eq!(dropped, 0);

    let (items2, new_cursor2, dropped2) = buf.drain_from(new_cursor, 3);
    assert_eq!(items2.len(), 2);
    assert_eq!(new_cursor2, 5);
    assert_eq!(dropped2, 0);
}

#[test]
fn drain_from_skips_already_consumed_cursor() {
    let mut buf = CanBuffer::new(10);
    for i in 0..3u32 {
        buf.push(mk_frame(u64::from(i), i));
    }
    // cursor=10,远超 version=3 → dropped=7
    let (items, new_cursor, dropped) = buf.drain_from(10, 5);
    assert_eq!(items.len(), 3);
    assert_eq!(dropped, 7);
    assert_eq!(new_cursor, 10);
}

#[test]
fn clear_keeps_version_monotonic() {
    let mut buf = CanBuffer::new(5);
    buf.push(mk_frame(1, 1));
    buf.push(mk_frame(2, 2));
    let v_before = buf.version();
    buf.clear();
    assert!(buf.is_empty());
    assert!(buf.version() > v_before, "版本号必须继续递增避免订阅漏数据");
}

#[test]
fn set_max_size_shrinks_buffer() {
    let mut buf = CanBuffer::new(10);
    for i in 0..8u32 {
        buf.push(mk_frame(u64::from(i), i));
    }
    buf.set_max_size(3);
    assert_eq!(buf.capacity(), 3);
    assert_eq!(buf.len(), 3);
}

#[test]
fn set_max_size_minimum_one() {
    let mut buf = CanBuffer::new(5);
    buf.set_max_size(0);
    assert_eq!(buf.capacity(), 1);
}

#[test]
fn capacity_at_least_one_when_constructed_with_zero() {
    let buf = CanBuffer::new(0);
    assert_eq!(buf.capacity(), 1, "capacity 必须至少为 1");
}
