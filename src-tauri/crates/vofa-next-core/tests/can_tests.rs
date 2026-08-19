//! CAN 模块集成测试

use vofa_next_core::can::{CanBuffer, CanFrame, CanLoadStats};
use vofa_next_core::CanDirection;

const fn make_frame(id: u32, data: Vec<u8>) -> CanFrame {
    CanFrame {
        timestamp: 0,
        id,
        extended: false,
        rtr: false,
        dlc: data.len() as u8,
        data,
        direction: CanDirection::Rx,
    }
}

#[test]
fn test_can_buffer_new_empty() {
    let buf = CanBuffer::new(10);
    assert_eq!(buf.len(), 0);
    assert!(buf.is_empty());
}

#[test]
fn test_can_buffer_push_and_len() {
    let mut buf = CanBuffer::new(10);
    buf.push(make_frame(0x100, vec![0x01]));
    assert_eq!(buf.len(), 1);
    assert!(!buf.is_empty());
    buf.push(make_frame(0x200, vec![0x02]));
    assert_eq!(buf.len(), 2);
}

#[test]
fn test_can_buffer_get_recent_basic() {
    let mut buf = CanBuffer::new(10);
    buf.push(make_frame(0x100, vec![0x01]));
    buf.push(make_frame(0x200, vec![0x02]));
    buf.push(make_frame(0x300, vec![0x03]));
    let recent = buf.get_recent(2);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].id, 0x200);
    assert_eq!(recent[1].id, 0x300);
}

#[test]
fn test_can_buffer_get_recent_returns_in_time_order() {
    let mut buf = CanBuffer::new(10);
    for i in 0..5u32 {
        buf.push(make_frame(0x100 + i, vec![i as u8]));
    }
    let recent = buf.get_recent(3);
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].id, 0x102);
    assert_eq!(recent[1].id, 0x103);
    assert_eq!(recent[2].id, 0x104);
}

#[test]
fn test_can_buffer_get_recent_count_greater_than_len() {
    let mut buf = CanBuffer::new(10);
    buf.push(make_frame(0x100, vec![0x01]));
    buf.push(make_frame(0x200, vec![0x02]));
    let recent = buf.get_recent(100);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].id, 0x100);
    assert_eq!(recent[1].id, 0x200);
}

#[test]
fn test_can_buffer_get_recent_zero() {
    let mut buf = CanBuffer::new(10);
    buf.push(make_frame(0x100, vec![0x01]));
    let recent = buf.get_recent(0);
    assert_eq!(recent.len(), 0);
}

#[test]
fn test_can_buffer_get_recent_empty_buffer() {
    let buf = CanBuffer::new(10);
    let recent = buf.get_recent(5);
    assert_eq!(recent.len(), 0);
}

#[test]
fn test_can_buffer_clear() {
    let mut buf = CanBuffer::new(10);
    buf.push(make_frame(0x100, vec![0x01]));
    buf.push(make_frame(0x200, vec![0x02]));
    assert_eq!(buf.len(), 2);
    buf.clear();
    assert_eq!(buf.len(), 0);
    assert!(buf.is_empty());
}

#[test]
fn test_can_buffer_clear_idempotent() {
    let mut buf = CanBuffer::new(10);
    buf.clear();
    buf.clear();
    assert!(buf.is_empty());
}

#[test]
fn test_can_buffer_overflow_drops_oldest() {
    let mut buf = CanBuffer::new(3);
    buf.push(make_frame(0x100, vec![0x01]));
    buf.push(make_frame(0x200, vec![0x02]));
    buf.push(make_frame(0x300, vec![0x03]));
    buf.push(make_frame(0x400, vec![0x04]));
    assert_eq!(buf.len(), 3);
    let all = buf.get_recent(3);
    assert_eq!(all[0].id, 0x200);
    assert_eq!(all[1].id, 0x300);
    assert_eq!(all[2].id, 0x400);
}

#[test]
fn test_can_buffer_overflow_preserves_recent() {
    let mut buf = CanBuffer::new(5);
    for i in 0..10u32 {
        buf.push(make_frame(0x100 + i, vec![i as u8]));
    }
    assert_eq!(buf.len(), 5);
    let recent = buf.get_recent(3);
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].id, 0x107);
    assert_eq!(recent[1].id, 0x108);
    assert_eq!(recent[2].id, 0x109);
}

#[test]
fn test_can_buffer_max_size_one() {
    let mut buf = CanBuffer::new(1);
    buf.push(make_frame(0x100, vec![0x01]));
    assert_eq!(buf.len(), 1);
    buf.push(make_frame(0x200, vec![0x02]));
    assert_eq!(buf.len(), 1);
    let recent = buf.get_recent(1);
    assert_eq!(recent[0].id, 0x200);
}

#[test]
fn test_can_buffer_preserves_frame_fields() {
    let mut buf = CanBuffer::new(10);
    let original = CanFrame {
        timestamp: 12345,
        id: 0x7FF,
        extended: true,
        rtr: true,
        dlc: 8,
        data: vec![0xDE, 0xAD, 0xBE, 0xEF, 0x12, 0x34, 0x56, 0x78],
        direction: CanDirection::Tx,
    };
    buf.push(original.clone());
    let recent = buf.get_recent(1);
    assert_eq!(recent.len(), 1);
    let f = &recent[0];
    assert_eq!(f.timestamp, original.timestamp);
    assert_eq!(f.id, original.id);
    assert_eq!(f.extended, original.extended);
    assert_eq!(f.rtr, original.rtr);
    assert_eq!(f.dlc, original.dlc);
    assert_eq!(f.data, original.data);
    assert_eq!(f.direction, original.direction);
}

// ===== CanLoadStats tests =====

fn make_load_frame(id: u32, dlc: u8, timestamp: u64) -> CanFrame {
    CanFrame {
        timestamp,
        id,
        extended: false,
        rtr: false,
        dlc,
        data: vec![0; dlc as usize],
        direction: CanDirection::Rx,
    }
}

#[test]
fn test_load_stats_empty() {
    let stats = CanLoadStats::new(1_000_000, 60);
    let snap = stats.snapshot(500_000);
    assert_eq!(snap.frame_count, 0);
    assert_eq!(snap.total_bits, 0);
    assert!(snap.load_ratio.abs() < 1e-9);
    assert!(snap.per_id.is_empty());
}

#[test]
fn test_load_stats_single_frame() {
    let mut stats = CanLoadStats::new(1_000_000, 60);
    let frame = make_load_frame(0x123, 4, 1_000_000);
    stats.push(&frame);
    let snap = stats.snapshot(500_000);
    assert_eq!(snap.frame_count, 1);
    assert_eq!(snap.total_bytes, 4);
    assert_eq!(snap.total_bits, 94);
    assert!((snap.load_ratio - 94.0 / 500_000.0).abs() < 1e-9);
    assert_eq!(snap.per_id.len(), 1);
    assert_eq!(snap.per_id[0].id, 0x123);
    assert_eq!(snap.per_id[0].frame_count, 1);
}

#[test]
fn test_load_stats_extended_frame_more_bits() {
    let mut stats = CanLoadStats::new(1_000_000, 60);
    let frame = CanFrame {
        timestamp: 1_000_000,
        id: 0x12345678,
        extended: true,
        rtr: false,
        dlc: 8,
        data: vec![0; 8],
        direction: CanDirection::Rx,
    };
    stats.push(&frame);
    let snap = stats.snapshot(500_000);
    assert_eq!(snap.total_bits, 157);
    assert!(snap.per_id[0].extended);
}

#[test]
fn test_load_stats_window_eviction() {
    let mut stats = CanLoadStats::new(100_000, 60);
    stats.push(&make_load_frame(0x100, 4, 100_000));
    stats.push(&make_load_frame(0x100, 4, 200_000));
    stats.push(&make_load_frame(0x100, 4, 300_000));
    let snap = stats.snapshot(500_000);
    assert_eq!(snap.frame_count, 2);
}

#[test]
fn test_load_stats_per_id_aggregation() {
    let mut stats = CanLoadStats::new(1_000_000, 60);
    stats.push(&make_load_frame(0x100, 4, 1_000_000));
    stats.push(&make_load_frame(0x100, 4, 1_100_000));
    stats.push(&make_load_frame(0x200, 8, 1_200_000));
    let snap = stats.snapshot(500_000);
    assert_eq!(snap.per_id.len(), 2);
    let id_100 = snap.per_id.iter().find(|s| s.id == 0x100).unwrap();
    assert_eq!(id_100.frame_count, 2);
    assert_eq!(id_100.total_bytes, 8);
    let id_200 = snap.per_id.iter().find(|s| s.id == 0x200).unwrap();
    assert_eq!(id_200.frame_count, 1);
    assert_eq!(id_200.total_bytes, 8);
}

#[test]
fn test_load_stats_history_sampling() {
    let mut stats = CanLoadStats::new(1_000_000, 3);
    stats.sample_history(500_000, 1_000_000);
    stats.sample_history(500_000, 1_100_000);
    stats.sample_history(500_000, 1_200_000);
    stats.sample_history(500_000, 1_300_000);
    let snap = stats.snapshot(500_000);
    assert_eq!(snap.history.len(), 3);
    assert_eq!(snap.history[0].timestamp, 1_100_000);
    assert_eq!(snap.history[2].timestamp, 1_300_000);
}

#[test]
fn test_load_stats_clear() {
    let mut stats = CanLoadStats::new(1_000_000, 60);
    stats.push(&make_load_frame(0x100, 4, 1_000_000));
    stats.sample_history(500_000, 1_000_000);
    stats.clear();
    let snap = stats.snapshot(500_000);
    assert_eq!(snap.frame_count, 0);
    assert!(snap.history.is_empty());
    assert!(snap.per_id.is_empty());
}

#[test]
fn test_load_stats_set_window_us() {
    let mut stats = CanLoadStats::new(1_000_000, 60);
    stats.push(&make_load_frame(0x100, 4, 1_000_000));
    stats.push(&make_load_frame(0x100, 4, 1_500_000));
    stats.set_window_us(200_000);
    let snap = stats.snapshot(500_000);
    assert_eq!(snap.frame_count, 1);
}

#[test]
fn test_load_stats_frame_bits_formula() {
    let f = make_load_frame(0x100, 0, 0);
    assert_eq!(CanLoadStats::frame_bits(&f), 56);
    let f = make_load_frame(0x100, 8, 0);
    assert_eq!(CanLoadStats::frame_bits(&f), 133);
    let f = CanFrame {
        timestamp: 0,
        id: 0x12345678,
        extended: true,
        rtr: false,
        dlc: 0,
        data: vec![],
        direction: CanDirection::Rx,
    };
    assert_eq!(CanLoadStats::frame_bits(&f), 80);
    let f = CanFrame {
        timestamp: 0,
        id: 0x12345678,
        extended: true,
        rtr: false,
        dlc: 8,
        data: vec![0; 8],
        direction: CanDirection::Rx,
    };
    assert_eq!(CanLoadStats::frame_bits(&f), 157);
}

#[test]
fn test_load_stats_per_id_history_sampling() {
    let mut stats = CanLoadStats::new(1_000_000, 5);
    stats.push(&make_load_frame(0x100, 4, 1_000_000));
    stats.push(&make_load_frame(0x200, 4, 1_000_000));
    stats.sample_history(500_000, 1_000_000);
    stats.sample_history(500_000, 1_100_000);
    let snap = stats.snapshot(500_000);
    assert_eq!(snap.per_id_history.len(), 2);
    for h in &snap.per_id_history {
        assert_eq!(h.history.len(), 2);
        assert!(h.id == 0x100 || h.id == 0x200);
    }
}

#[test]
fn test_load_stats_per_id_history_capacity() {
    let mut stats = CanLoadStats::new(1_000_000, 3);
    stats.push(&make_load_frame(0x100, 4, 1_000_000));
    for i in 0..5u64 {
        stats.sample_history(500_000, 1_000_000 + i * 100_000);
    }
    let snap = stats.snapshot(500_000);
    assert_eq!(snap.per_id_history.len(), 1);
    assert_eq!(snap.per_id_history[0].history.len(), 3);
    assert_eq!(snap.per_id_history[0].history[0].timestamp, 1_200_000);
    assert_eq!(snap.per_id_history[0].history[2].timestamp, 1_400_000);
}

#[test]
fn test_load_stats_per_id_history_clear() {
    let mut stats = CanLoadStats::new(1_000_000, 5);
    stats.push(&make_load_frame(0x100, 4, 1_000_000));
    stats.sample_history(500_000, 1_000_000);
    stats.clear();
    let snap = stats.snapshot(500_000);
    assert!(snap.per_id_history.is_empty());
}

#[test]
fn test_load_stats_per_id_history_eviction() {
    let mut stats = CanLoadStats::new(100_000, 5);
    stats.push(&make_load_frame(0x100, 4, 100_000));
    stats.sample_history(500_000, 100_000);
    stats.push(&make_load_frame(0x200, 4, 300_000));
    stats.sample_history(500_000, 300_000);
    let snap = stats.snapshot(500_000);
    assert!(snap.per_id.iter().all(|s| s.id != 0x100));
    assert!(snap.per_id_history.iter().all(|h| h.id != 0x100));
    assert!(snap.per_id_history.iter().any(|h| h.id == 0x200));
}

#[test]
fn test_load_stats_per_id_history_load_ratio() {
    let mut stats = CanLoadStats::new(1_000_000, 5);
    stats.push(&make_load_frame(0x100, 4, 1_000_000));
    stats.sample_history(500_000, 1_000_000);
    let snap = stats.snapshot(500_000);
    let h = &snap.per_id_history[0];
    assert!((h.history[0].load_ratio - 94.0 / 500_000.0).abs() < 1e-9);
}
