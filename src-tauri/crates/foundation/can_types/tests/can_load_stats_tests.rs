//! `can_types::can_load_stats` 单元测试 + `frame_bits` 工具测试

use can_types::{frame_bits, CanDirection, CanFrame, CanFrameTestData, CanLoadStats};

// ============ frame_bits 测试 ============

#[test]
fn frame_bits_standard_at_dlc_eight() {
    // standard: (47 + 8*8) * 1.2 = (47+64)*1.2 = 133.2 → 133
    let f = CanFrame {
        timestamp: 0,
        id: 0x100,
        extended: false,
        rtr: false,
        dlc: 8,
        data: vec![0; 8],
        direction: CanDirection::Rx,
    };
    assert_eq!(frame_bits(&f), 133);
}

#[test]
fn frame_bits_standard_at_dlc_zero() {
    // standard DLC=0: 47 * 1.2 = 56.4 → 56
    let f = CanFrame::new(0, 0x100, vec![], CanDirection::Rx);
    assert_eq!(frame_bits(&f), 56);
}

#[test]
fn frame_bits_extended_is_larger_than_standard() {
    let std = CanFrame::new(0, 0x100, vec![1, 2, 3], CanDirection::Rx);
    let mut ext = std.clone();
    ext.extended = true;
    assert!(frame_bits(&ext) > frame_bits(&std));
}

#[test]
fn frame_bits_dlc_grows_monotonically() {
    let mut prev = 0;
    for dlc in 0..=8u8 {
        let f = CanFrame::new(0, 0x100, vec![0; dlc as usize], CanDirection::Rx);
        let bits = frame_bits(&f);
        if dlc > 0 {
            assert!(
                bits > prev,
                "DLC={dlc} bits={bits} should exceed prev={prev}"
            );
        }
        prev = bits;
    }
}

// ============ CanLoadStats 测试 ============

fn build_stats(window_us: u64, history_cap: usize) -> CanLoadStats {
    CanLoadStats::new(window_us, history_cap)
}

#[test]
fn new_stats_window_us_clamped_to_one_minimum() {
    let s = build_stats(0, 100);
    assert_eq!(s.window_us(), 1);
}

#[test]
fn push_single_frame_updates_totals() {
    let mut s = build_stats(1_000_000, 100);
    let f = CanFrameTestData::load_frame(0x100, 8, 1000);
    s.push(&f);
    // load_ratio 基于当前窗口内的 total_bits 计算,非 0
    let r = s.load_ratio(500_000);
    assert!(r > 0.0);
    assert!(r < 1.0);
}

#[test]
fn load_ratio_increases_with_dense_traffic() {
    let mut s = build_stats(1_000_000, 100);
    // 1s 窗口,500kbps → 500_000 bits/s = 500_000_000 bits/窗口
    let standard_bits = u64::from(frame_bits(&CanFrame::new(
        0,
        0x100,
        vec![0; 8],
        CanDirection::Rx,
    )));
    // 推 100 帧 100ms 内 (然后 sample_history) → 高负载
    let frames = CanFrameTestData::standard_frames(0x100, 100);
    for (i, f) in frames.iter().enumerate() {
        s.push(&{
            let mut g = f.clone();
            g.timestamp = i as u64 * 10_000;
            g
        });
    }
    s.sample_history(500_000, 1_000_000);
    let snap = s.snapshot(500_000);
    assert!(snap.frame_count > 0);
    assert!(snap.load_ratio > 0.0);
    let _ = standard_bits;
}

#[test]
fn evict_removes_old_samples_on_push() {
    let mut s = build_stats(1_000_000, 100);
    for i in 0..10u32 {
        s.push(&CanFrameTestData::load_frame(
            0x100 + i,
            0,
            u64::from(i) * 100,
        ));
    }
    // 推到第 1_100_000us 时,前面 9 个样本 (ts<100_000) 已被剔除
    s.push(&CanFrameTestData::load_frame(0x200, 0, 1_100_000));
    s.sample_history(500_000, 1_100_000);
    // 剩余 frame_count 应只有 1 个 (新推入的)
    assert_eq!(s.snapshot(500_000).frame_count, 1);
}

#[test]
fn set_window_us_shrinks_and_evicts() {
    let mut s = build_stats(1_000_000, 100);
    for i in 0..5 {
        s.push(&CanFrameTestData::load_frame(0x100, 0, i * 100_000));
    }
    // 缩窗至 200_000us,所有 t<[latest-200_000] 都应被剔
    s.set_window_us(200_000);
    let snap = s.snapshot(500_000);
    assert_eq!(snap.window_us, 200_000);
    assert!(snap.frame_count <= 3);
}

#[test]
fn per_id_distribution_tracks_independent_ids() {
    let mut s = build_stats(1_000_000, 100);
    // 5 帧共享 id=0x100
    for _ in 0..5 {
        s.push(&CanFrameTestData::load_frame(0x100, 8, 0));
    }
    // 3 帧共享 id=0x200
    for _ in 0..3 {
        s.push(&CanFrameTestData::load_frame(0x200, 8, 0));
    }
    let snap = s.snapshot(500_000);
    assert_eq!(snap.per_id.len(), 2);
    // 按 total_bits 降序排列 (帧数多者居前)
    assert_eq!(snap.per_id[0].id, 0x100);
    assert_eq!(snap.per_id[0].frame_count, 5);
    assert_eq!(snap.per_id[1].id, 0x200);
    assert_eq!(snap.per_id[1].frame_count, 3);
}

#[test]
fn per_id_history_is_sampled_on_sample_history() {
    let mut s = build_stats(1_000_000, 100);
    s.push(&CanFrameTestData::load_frame(0x42, 4, 0));
    s.sample_history(500_000, 1_000);
    let snap = s.snapshot(500_000);
    assert_eq!(snap.per_id_history.len(), 1);
    assert!(!snap.per_id_history[0].history.is_empty());
}

#[test]
fn load_ratio_zero_when_no_bitrate() {
    let s = build_stats(1_000_000, 100);
    assert!(s.load_ratio(0).abs() < 1e-6);
}

#[test]
fn fps_is_nan_or_zero_when_window_zero() {
    let s = CanLoadStats::new(1, 100);
    // window_us=1,无可用样本
    let _ = s.fps(); // 仅需不 panic
}

#[test]
fn clear_resets_all_state() {
    let mut s = build_stats(1_000_000, 100);
    s.push(&CanFrameTestData::load_frame(0x42, 8, 0));
    s.sample_history(500_000, 1_000);
    s.clear();
    let snap = s.snapshot(500_000);
    assert_eq!(snap.frame_count, 0);
    assert_eq!(snap.total_bits, 0);
}

#[test]
fn history_capacity_truncates() {
    let mut s = build_stats(1_000_000, 3);
    s.push(&CanFrameTestData::load_frame(0x42, 0, 0));
    for i in 0..10 {
        s.sample_history(500_000, 1_000 * (i + 1));
    }
    let snap = s.snapshot(500_000);
    assert!(snap.history.len() <= 3);
}

#[test]
fn snapshot_per_id_sorted_by_total_bits_desc() {
    let mut s = build_stats(1_000_000, 100);
    // 给 id=2 推更多帧
    for _ in 0..10 {
        s.push(&CanFrameTestData::load_frame(0x200, 8, 0));
    }
    for _ in 0..3 {
        s.push(&CanFrameTestData::load_frame(0x100, 8, 0));
    }
    let snap = s.snapshot(500_000);
    assert_eq!(snap.per_id.len(), 2);
    assert!(snap.per_id[0].total_bits >= snap.per_id[1].total_bits);
}
