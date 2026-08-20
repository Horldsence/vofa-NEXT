//! `logic_types::buffers` 单元测试
//!
//! 覆盖:
//! - LogicBuffer / DecodedBuffer push、get_recent、drain_from 行为
//! - 版本号单调性
//! - 容量上限与 set_max_size

use logic_types::{
    DecodedBuffer, DecodedEvent, I2cEvent, LogicBuffer, LogicSample,
};

fn sample(ts: u64) -> LogicSample {
    LogicSample::new(ts, 0xFF & (ts as u32), 8)
}

fn uart(ts: u64, byte: u8) -> DecodedEvent {
    DecodedEvent::Uart {
        timestamp: ts,
        byte,
        parity_ok: true,
    }
}

fn i2c(ts: u64) -> DecodedEvent {
    DecodedEvent::I2c {
        timestamp: ts,
        event: I2cEvent::Stop,
    }
}

#[test]
fn logic_buffer_new_is_empty() {
    let b = LogicBuffer::new(10);
    assert!(b.is_empty());
    assert_eq!(b.len(), 0);
    assert_eq!(b.version(), 0);
    assert_eq!(b.max_size(), 10);
}

#[test]
fn logic_buffer_push_increments_version() {
    let mut b = LogicBuffer::new(5);
    b.push(sample(1));
    assert_eq!(b.version(), 1);
    b.push(sample(2));
    assert_eq!(b.version(), 2);
}

#[test]
fn logic_buffer_push_past_capacity_drops_oldest() {
    let mut b = LogicBuffer::new(3);
    b.push(sample(1));
    b.push(sample(2));
    b.push(sample(3));
    b.push(sample(4));
    assert_eq!(b.len(), 3);
    let recent = b.get_recent(10);
    assert_eq!(recent.iter().map(|s| s.timestamp).collect::<Vec<_>>(), vec![2, 3, 4]);
}

#[test]
fn logic_buffer_get_recent_chronological() {
    let mut b = LogicBuffer::new(10);
    for ts in 0..5 {
        b.push(sample(ts));
    }
    let recent = b.get_recent(3);
    assert_eq!(recent[0].timestamp, 2);
    assert_eq!(recent[2].timestamp, 4);
}

#[test]
fn logic_buffer_drain_from_normal() {
    let mut b = LogicBuffer::new(10);
    for ts in 0..5 {
        b.push(sample(ts));
    }
    // version=5, frames=5
    let (items, new_cursor, dropped) = b.drain_from(0, 3);
    assert_eq!(items.len(), 3);
    assert_eq!(new_cursor, 3);
    assert_eq!(dropped, 0);
}

#[test]
fn logic_buffer_drain_from_cursor_ahead() {
    let mut b = LogicBuffer::new(10);
    for ts in 0..3 {
        b.push(sample(ts));
    }
    // cursor=10 远超 version=3
    let (items, new_cursor, dropped) = b.drain_from(10, 5);
    assert_eq!(items.len(), 3);
    assert_eq!(new_cursor, 10);
    assert_eq!(dropped, 7);
}

#[test]
fn logic_buffer_clear_keeps_version_monotonic() {
    let mut b = LogicBuffer::new(5);
    b.push(sample(1));
    let v_before = b.version();
    b.clear();
    assert!(b.is_empty());
    assert!(b.version() > v_before);
}

#[test]
fn logic_buffer_set_max_size_shrinks() {
    let mut b = LogicBuffer::new(10);
    for ts in 0..8 {
        b.push(sample(ts));
    }
    b.set_max_size(3);
    assert_eq!(b.max_size(), 3);
    assert_eq!(b.len(), 3);
}

#[test]
fn logic_buffer_set_max_size_min_one() {
    let mut b = LogicBuffer::new(5);
    b.set_max_size(0);
    assert_eq!(b.max_size(), 1);
}

#[test]
fn decoded_buffer_new_is_empty() {
    let b = DecodedBuffer::new(10);
    assert!(b.is_empty());
    assert_eq!(b.len(), 0);
    assert_eq!(b.max_size(), 10);
}

#[test]
fn decoded_buffer_push_increments_version() {
    let mut b = DecodedBuffer::new(5);
    b.push(uart(1, 0x10));
    assert_eq!(b.version(), 1);
    b.push(i2c(2));
    assert_eq!(b.version(), 2);
}

#[test]
fn decoded_buffer_push_past_capacity_drops_oldest() {
    let mut b = DecodedBuffer::new(3);
    b.push(uart(1, 0));
    b.push(uart(2, 0));
    b.push(uart(3, 0));
    b.push(uart(4, 0));
    assert_eq!(b.len(), 3);
}

#[test]
fn decoded_buffer_get_recent_chronological() {
    let mut b = DecodedBuffer::new(10);
    for ts in 0..5 {
        b.push(uart(ts, 0xAA));
    }
    let recent = b.get_recent(3);
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].timestamp(), 2);
    assert_eq!(recent[2].timestamp(), 4);
}

#[test]
fn decoded_buffer_drain_from_normal_and_ahead() {
    let mut b = DecodedBuffer::new(10);
    for ts in 0..5 {
        b.push(uart(ts, 0xAA));
    }
    let (items, new_cursor, dropped) = b.drain_from(0, 5);
    assert_eq!(items.len(), 5);
    assert_eq!(new_cursor, 5);
    assert_eq!(dropped, 0);

    let (items2, _, dropped2) = b.drain_from(100, 3);
    assert_eq!(items2.len(), 5);
    assert_eq!(dropped2, 95);
}

#[test]
fn decoded_buffer_clear_keeps_version_monotonic() {
    let mut b = DecodedBuffer::new(5);
    b.push(uart(1, 0));
    let v_before = b.version();
    b.clear();
    assert!(b.is_empty());
    assert!(b.version() > v_before);
}

#[test]
fn decoded_buffer_set_max_size_min_one() {
    let mut b = DecodedBuffer::new(5);
    b.set_max_size(0);
    assert_eq!(b.max_size(), 1);
}
