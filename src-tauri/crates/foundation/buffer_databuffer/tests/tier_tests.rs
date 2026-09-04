#![allow(clippy::option_if_let_else)]
#![allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // 测试数值断言: 精确比较 + 小整数 cast 均有界
use buffer_databuffer::{DataBuffer, WaveformSampling, WaveformSeriesSelection};

fn all_channels(count: usize) -> WaveformSeriesSelection {
    WaveformSeriesSelection {
        channels: (0..count).collect(),
        derived: vec![],
    }
}

#[test]
fn memory_estimate_includes_allocated_pyramid_columns() {
    let mut buffer = DataBuffer::new(1_000, 4);
    let raw = buffer.estimated_bytes();
    assert_eq!(raw, 1_000 * (8 + 4 * 4));
    for i in 0..16_u64 {
        buffer.push_frame_at(i, &[1.0; 4]);
    }
    assert_eq!(buffer.estimated_bytes() - raw, 4_096 * 2 * (8 + 4 * 4));
}

/// 正弦 + 已知极值尖峰; L0 只装得下 1/5 历史, 预算窗口查询必须从金字塔层
/// 取出真实包络: 尖峰不丢、时间覆盖完整 (不变量 2: 旧数据降质不消失)
#[test]
fn pyramid_preserves_extrema_beyond_l0_span() {
    let mut buf = DataBuffer::new(1_000, 1);
    let total = 5_000_usize; // 1kHz × 5s; L0 只留最后 1s
    for i in 0..total {
        let ts = u64::try_from(i).unwrap_or(0) * 1_000;
        let v = if i == 100 {
            1_000.0_f32 // 高尖峰 (L0 早已覆盖不住)
        } else if i == 2_000 {
            -900.0 // 低尖峰
        } else {
            (i as f64 * 0.02).sin().mul_add(50.0, 128.0) as f32
        };
        buf.push_frame_at(ts, &[v]);
    }
    assert_eq!(buf.max_points(), 1_000);
    assert!(buf.storage_overflow() > 0, "L0 滚动覆盖应被计数");
    // 折叠确实发生 (L1 至少应有 300+ 块)
    assert!(buf.point_count() == 1_000, "L0 应保持满环");

    // 窗口 [-5s, 0] 远超 L0 覆盖 (1s), 预算 200 → 必须走金字塔层
    let selection = all_channels(1);
    let w = buf.snapshot_window_budget(-5_000.0, 0.0, &selection, 200);
    assert!(
        w.raw_window_points() > 1_000,
        "raw_window_points 应反映窗口真实原始点数: {}",
        w.raw_window_points()
    );
    let window = w.into_min_max(200);
    assert_eq!(window.sampling, WaveformSampling::MinMax, "层快照非 Raw");
    // 时间覆盖: 最早点应远早于 L0 能提供的 -1s
    assert!(
        window.timestamps.first().copied().unwrap_or(0.0) < -3_000.0,
        "金字塔层应补全 L0 之外的历史: 首点 {:?}",
        window.timestamps.first()
    );
    let ch = &window.channels[0];
    assert!(
        ch.contains(&1_000.0),
        "高尖峰必须出现在包络中 (min-max 保真): max={:?}",
        ch.iter().copied().fold(f32::MIN, f32::max)
    );
    assert!(
        ch.contains(&-900.0),
        "低尖峰必须出现在包络中: min={:?}",
        ch.iter().copied().fold(f32::MAX, f32::min)
    );
    // 包络范围正确 (无串通道/错位)
    assert!(ch.iter().copied().all(f32::is_finite));
}

/// 预算窗口查询的分层选择: 窗口小 (L0 覆盖内) 且点数不超预算 → 原始快照
#[test]
fn small_window_inside_l0_stays_raw() {
    let mut buf = DataBuffer::new(1_000, 1);
    for i in 0..2_000_u64 {
        buf.push_frame_at(i * 1_000, &[(i % 7) as f32]);
    }
    let selection = all_channels(1);
    // 最近 10ms → 含两端 11 点 (端点闭区间), L0 覆盖内
    let w = buf.snapshot_window_budget(-10.0, 0.0, &selection, 1_000);
    assert_eq!(w.raw_window_points(), 11);
    let window = w.into_min_max(1_000);
    assert_eq!(window.sampling, WaveformSampling::Raw);
    assert_eq!(window.timestamps.len(), 11);
}

/// 容量自洽: 按速率×窗口整备, 封顶生效, 幂等
#[test]
fn capacity_autotune_grows_to_rate_times_window_and_caps() {
    let mut buf = DataBuffer::new(100_000, 3);
    // 700kHz × 2.5s = 1.75M 点
    assert!(buf.ensure_capacity_for_rate(700_000.0, 2.5, 4_000_000));
    assert_eq!(buf.max_points(), 1_750_000);
    // 幂等: 已达标不动作
    assert!(!buf.ensure_capacity_for_rate(700_000.0, 2.5, 4_000_000));
    // 封顶: 目标超上限 → 收敛到 cap
    assert!(buf.ensure_capacity_for_rate(7_000_000.0, 2.5, 4_000_000));
    assert_eq!(buf.max_points(), 4_000_000);
    // 已超 cap 时不缩减 (只增不减, 用户手动 set_max_points 才缩)
    assert!(!buf.ensure_capacity_for_rate(700_000.0, 2.5, 4_000_000));
}

/// 溢出计数: L0 满后每次覆盖 +1 (不变量 5: 丢弃显式化)
#[test]
fn storage_overflow_counts_evictions() {
    let mut buf = DataBuffer::new(10, 1);
    for i in 0..25_u64 {
        buf.push_frame_at(i * 100, &[i as f32]);
    }
    assert_eq!(buf.point_count(), 10);
    assert_eq!(buf.storage_overflow(), 15);
}

/// 金字塔全历史概览: snapshot_all_budget 在层间选择, 覆盖 L0 之外的历史
#[test]
fn overview_budget_serves_full_history_from_tiers() {
    let mut buf = DataBuffer::new(500, 1);
    for i in 0..4_000_u64 {
        let v = if i == 1_234 { 777.0 } else { (i % 13) as f32 };
        buf.push_frame_at(i * 1_000, &[v]);
    }
    // 概览预算 100: L2 = 4000/256 ≈ 15 块 ≈ 31 点 ≤ 400 → 命中高层
    let window = buf.snapshot_all_budget(100).into_min_max(100);
    assert!(window.timestamps.len() > 5);
    assert_eq!(window.sampling, WaveformSampling::MinMax);
    assert!(
        window.channels[0].contains(&777.0),
        "全历史概览必须保留极值"
    );
    // 首点时间应接近全历史起点 (i=0 → 0ms → 相对 latest ≈ -3999ms)
    assert!(
        window.timestamps.first().copied().unwrap_or(0.0) < -3_000.0,
        "概览应覆盖 L0 之外的历史: {:?}",
        window.timestamps.first()
    );
}

/// 回归: 极值顺序不等于时间顺序时，旧实现会写出 (min_ts, max_ts) 的倒序 X 轴，
/// 二分窗口与前端概览都会随滚动发生回跳。
#[test]
fn pyramid_timestamps_are_monotonic_for_reversed_extrema() {
    let mut buf = DataBuffer::new(64, 2);
    for i in 0..1_024_u64 {
        let within_block = (i % 16) as f32;
        // CH0 每块从高到低，min 的时刻晚于 max；CH1 方向相反。
        buf.push_frame_at(i * 10, &[15.0 - within_block, within_block]);
    }

    let window = buf.snapshot_all_budget(128).into_min_max(128);
    assert!(window.buffer_tier > 0, "测试必须命中金字塔层");
    assert!(
        window.timestamps.windows(2).all(|pair| pair[0] <= pair[1]),
        "金字塔公共时间轴不得回跳: {:?}",
        window.timestamps
    );
}

/// 回归: 金字塔必须使用独立于任一通道极值位置的公共 X 轴。每个块的 min/max
/// 共用块时间戳，避免把 CH0 的极值时刻借给其他通道而造成整体抖动或通道错位。
#[test]
fn pyramid_uses_one_shared_timestamp_pair_per_block() {
    let mut buf = DataBuffer::new(64, 2);
    for i in 0..32_u64 {
        let within_block = (i % 16) as f32;
        buf.push_frame_at(i * 100, &[15.0 - within_block, within_block]);
    }

    let window = buf.snapshot_all_budget(1_000).into_min_max(1_000);
    assert_eq!(window.buffer_tier, 1);
    assert_eq!(window.timestamps.len(), 4);
    assert_eq!(window.timestamps[0], window.timestamps[1]);
    assert_eq!(window.timestamps[2], window.timestamps[3]);
    assert!(window.timestamps[1] < window.timestamps[2]);
    assert_eq!(window.channels[0], vec![0.0, 15.0, 0.0, 15.0]);
    assert_eq!(window.channels[1], vec![0.0, 15.0, 0.0, 15.0]);
}

/// 回归: 概览应选“满足预算的最细层”。倒序选择会在新高层刚创建时突然只返回
/// 两个点，表现为概览条运行一段时间后塌缩/消失。
#[test]
fn overview_selects_finest_tier_that_fits_budget() {
    let mut buf = DataBuffer::new(64, 1);
    for i in 0..4_096_u64 {
        buf.push_frame_at(i * 10, &[(i % 31) as f32]);
    }

    // L1: 512 条目 > 400；L2: 32 条目 <= 400；L3 刚生成仅 2 条目。
    let window = buf.snapshot_all_budget(100).into_min_max(100);
    assert_eq!(window.buffer_tier, 2, "应选择满足预算的最细层 L2");
    assert!(window.timestamps.len() > 2, "概览不能塌缩到刚生成的最粗层");
}
