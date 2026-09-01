#![allow(clippy::float_cmp, clippy::cast_precision_loss)] // 测试数值断言: 精确比较 + 小整数转 f32 无精度问题
use buffer_databuffer::{
    DataBuffer, DerivedSeriesSelector, WaveformSampling, WaveformSeriesSelection, WaveformWindow,
};
use vofa_core::DataFrame;

fn derived_values<'a>(window: &'a WaveformWindow, sink: &str, source: &str) -> &'a Vec<f32> {
    &window.derived[sink][source][""]
}

#[test]
fn get_window_with_derived() {
    let mut buf = DataBuffer::new(100, 1);
    for i in 0..5 {
        buf.push_frame(&DataFrame::new(vec![i as f32]));
        buf.push_derived("wave1", "math1", (i * 10) as f32);
    }
    let w = buf.get_recent(5);
    assert_eq!(w.channels[0], vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    let derived = derived_values(&w, "wave1", "math1");
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
fn waveform_window_json_represents_non_finite_gaps_as_null() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame(&DataFrame::new(vec![f32::NAN]));
    buf.push_derived("wave1", "math1", f32::INFINITY);
    let w = buf.get_recent(1);
    let json = serde_json::to_value(&w).unwrap();

    assert!(json["channels"][0][0].is_null());
    assert!(json["derived"]["wave1"]["math1"][""][0].is_null());
    assert_eq!(json["sampling"], "raw");
}

#[test]
fn get_recent_count_exceeds_buffer_returns_all() {
    let mut buf = DataBuffer::new(100, 1);
    buf.push_frame(&DataFrame::new(vec![1.0]));
    buf.push_frame(&DataFrame::new(vec![2.0]));
    let w = buf.get_recent(10);
    assert_eq!(w.channels[0].len(), 2);
}

#[test]
fn timestamps_keep_sub_millisecond_precision() {
    let mut buf = DataBuffer::new(10, 1);
    for timestamp in 10_000..10_003 {
        buf.push_frame(&DataFrame::with_timestamp(timestamp, vec![1.0]));
    }

    assert_eq!(buf.get_recent(3).timestamps, vec![-0.002, -0.001, 0.0]);
}

#[test]
fn min_max_window_spans_history_and_preserves_extrema() {
    let mut buf = DataBuffer::new(100, 2);
    for index in 0..100 {
        let ch0 = if index == 17 { 500.0 } else { index as f32 };
        let ch1 = if index == 73 { -400.0 } else { -(index as f32) };
        buf.push_frame(&DataFrame::with_timestamp(index * 1_000, vec![ch0, ch1]));
    }

    let window = buf.get_min_max(20);
    assert!(window.timestamps.len() <= 20);
    assert_eq!(window.timestamps.first(), Some(&-99.0));
    assert_eq!(window.timestamps.last(), Some(&0.0));
    assert!(window.channels[0].contains(&500.0));
    assert!(window.channels[1].contains(&-400.0));
    assert!(window
        .channels
        .iter()
        .all(|channel| channel.len() == window.timestamps.len()));
    assert_eq!(window.sampling, WaveformSampling::MinMax);
}

#[test]
fn min_max_never_discards_series_extrema_to_meet_an_impossible_budget() {
    let mut buf = DataBuffer::new(100, 1);
    for index in 0..50 {
        buf.push_frame(&DataFrame::with_timestamp(index * 1_000, vec![0.0]));
        for series in 0..12 {
            let value = if index == series * 3 + 1 {
                100.0 + series as f32
            } else {
                0.0
            };
            buf.push_derived("wave", &format!("math-{series}"), value);
        }
    }

    let window = buf.get_min_max(8);
    assert!(window.timestamps.len() > 8);
    for series in 0..12 {
        assert!(window.derived["wave"][&format!("math-{series}")][""]
            .contains(&(100.0 + series as f32)));
    }
    assert!(window
        .channels
        .iter()
        .all(|channel| channel.len() == window.timestamps.len()));
    assert!(window
        .derived
        .values()
        .flat_map(std::collections::HashMap::values)
        .flat_map(std::collections::HashMap::values)
        .all(|series| series.len() == window.timestamps.len()));
}

#[test]
fn sub_millisecond_700khz_window_uses_fractional_milliseconds() {
    let mut buf = DataBuffer::new(2_000, 1);
    for index in 0..1_000_u64 {
        let timestamp = (index as f64 * 1_000_000.0 / 700_000.0).round() as u64;
        buf.push_frame(&DataFrame::with_timestamp(timestamp, vec![index as f32]));
    }

    let window = buf.get_window_raw(-0.1, 0.0);
    assert!((70..=72).contains(&window.raw_window_points));
    assert_eq!(window.timestamps.last(), Some(&0.0));
    assert!(window
        .timestamps
        .first()
        .is_some_and(|value| *value >= -0.1));
}

#[test]
fn relative_windows_follow_independent_horizontal_positions() {
    let mut buf = DataBuffer::new(100, 1);
    for index in 0..100_u64 {
        buf.push_frame(&DataFrame::with_timestamp(
            index * 1_000,
            vec![index as f32],
        ));
    }

    let latest = buf.get_window_raw(-10.0, 0.0);
    let older = buf.get_window_raw(-50.0, -40.0);
    assert_eq!(latest.channels[0].first(), Some(&89.0));
    assert_eq!(latest.channels[0].last(), Some(&99.0));
    assert_eq!(older.channels[0].first(), Some(&49.0));
    assert_eq!(older.channels[0].last(), Some(&59.0));
}

#[test]
fn duplicate_timestamps_and_empty_windows_have_exact_bounds() {
    let mut buf = DataBuffer::new(10, 1);
    for (timestamp, value) in [(1_000, 1.0), (2_000, 2.0), (2_000, 3.0), (3_000, 4.0)] {
        buf.push_frame(&DataFrame::with_timestamp(timestamp, vec![value]));
    }

    let duplicates = buf.get_window_raw(-1.0, -1.0);
    assert_eq!(duplicates.channels[0], vec![2.0, 3.0]);
    assert_eq!(duplicates.timestamps, vec![-1.0, -1.0]);
    assert!(buf.get_window_raw(-10.0, -9.0).timestamps.is_empty());
}

#[test]
fn selected_derived_envelope_preserves_nan_safe_extrema_and_endpoints() {
    let mut buf = DataBuffer::new(200, 1);
    for index in 0..200_u64 {
        buf.push_frame(&DataFrame::with_timestamp(index * 10, vec![f32::NAN]));
        let value = match index {
            73 => 900.0,
            74 => -800.0,
            _ => f32::NAN,
        };
        buf.push_derived("wave", "math", value);
    }
    let selection = WaveformSeriesSelection {
        channels: vec![],
        derived: vec![DerivedSeriesSelector {
            sink_id: "wave".into(),
            source_id: "math".into(),
            source_handle: String::new(),
        }],
    };

    let window = buf.get_window_min_max(-10.0, 0.0, 20, &selection);
    let derived = derived_values(&window, "wave", "math");
    assert!(window.timestamps.len() <= 20);
    assert_eq!(window.timestamps.first(), Some(&-1.99));
    assert_eq!(window.timestamps.last(), Some(&0.0));
    assert!(derived.contains(&900.0));
    assert!(derived.contains(&-800.0));
    assert_eq!(window.sampling, WaveformSampling::MinMax);
}

#[test]
fn paused_lttb_keeps_endpoints_and_dominant_turning_point() {
    let mut buf = DataBuffer::new(1_000, 1);
    for index in 0..1_000_u64 {
        let value = if index == 501 {
            10_000.0
        } else {
            index as f32 * 0.001
        };
        buf.push_frame(&DataFrame::with_timestamp(index * 10, vec![value]));
    }
    let selection = WaveformSeriesSelection {
        channels: vec![0],
        derived: vec![],
    };

    let window = buf.get_window_lttb(-10.0, 0.0, 64, &selection);

    assert!(window.timestamps.len() <= 64);
    assert_eq!(window.timestamps.first(), Some(&-9.99));
    assert_eq!(window.timestamps.last(), Some(&0.0));
    assert!(window.channels[0].contains(&10_000.0));
    assert_eq!(window.sampling, WaveformSampling::Lttb);
}

/// 700k sps 逐样本时间戳 (restamp 后的真实形态) 下, min-max 窗口每通道仍应
/// 输出数千点 — 若这里退化为几十点, 说明阶梯来自算法而非时间戳
#[test]
fn min_max_high_rate_smooth_signal_stays_dense() {
    // 200ms 窗口 × 700k sps = 140_000 点, 4 通道平滑正弦, 逐样本时间戳
    let mut buf = DataBuffer::new(200_000, 4);
    for index in 0..140_000_u64 {
        let timestamp = (index as f64 * 1_000_000.0 / 700_000.0).round() as u64;
        let t = index as f64 / 700_000.0;
        let channels = (0..4)
            .map(|ch| {
                let freq = 1.0 + ch as f64;
                ((t * freq * 2.0 * std::f64::consts::PI).sin() * (1.0 + ch as f64 * 0.5) * 50.0
                    + 128.0) as f32
            })
            .collect();
        buf.push_frame(&DataFrame::with_timestamp(timestamp, channels));
    }
    let selection = WaveformSeriesSelection {
        channels: vec![0, 1, 2, 3],
        derived: vec![],
    };

    let window = buf.get_window_min_max(-200.0, 0.0, 8_800, &selection);

    assert_eq!(window.raw_window_points, 140_000);
    // 每通道约 2 × bucket_count 点; 若每通道不足 500 点则渲染必然呈阶梯
    for (ch, series) in window.channels.iter().enumerate() {
        assert!(
            series.len() >= 500,
            "CH{ch} 只输出 {} 点 (共 {} 时间列), min-max 在高码率下退化为阶梯",
            series.len(),
            window.timestamps.len()
        );
    }
}

#[test]
fn snapshot_clone_remains_frozen_after_live_ring_wraps() {
    let mut live = DataBuffer::new(4, 1);
    for index in 0..4_u64 {
        live.push_frame(&DataFrame::with_timestamp(index, vec![index as f32]));
    }
    let snapshot = live.clone();
    for index in 4..12_u64 {
        live.push_frame(&DataFrame::with_timestamp(index, vec![index as f32]));
    }

    assert_eq!(
        snapshot.get_recent(10).channels[0],
        vec![0.0, 1.0, 2.0, 3.0]
    );
    assert_eq!(live.get_recent(10).channels[0], vec![8.0, 9.0, 10.0, 11.0]);
}

#[test]
fn raw_csv_matches_selected_original_series() {
    let mut buf = DataBuffer::new(10, 2);
    for index in 0..3_u64 {
        buf.push_frame(&DataFrame::with_timestamp(
            10_000 + index,
            vec![index as f32, (index + 10) as f32],
        ));
        buf.push_derived("wave", "math", (index + 20) as f32);
    }
    let selection = WaveformSeriesSelection {
        channels: vec![1],
        derived: vec![DerivedSeriesSelector {
            sink_id: "wave".into(),
            source_id: "math".into(),
            source_handle: String::new(),
        }],
    };
    let mut csv = Vec::new();
    let rows = buf
        .write_raw_csv(&mut csv, 10_000, 10_002, &selection)
        .unwrap();

    assert_eq!(rows, 3);
    assert_eq!(
        String::from_utf8(csv).unwrap(),
        "timestamp_us,CH1,\"wave:math:\"\n10000,10,20\n10001,11,21\n10002,12,22\n"
    );
}

#[test]
fn raw_csv_writes_non_finite_gaps_as_empty_cells() {
    let mut buf = DataBuffer::new(10, 1);
    buf.push_frame(&DataFrame::with_timestamp(10_000, vec![f32::NAN]));
    let mut csv = Vec::new();
    let selection = WaveformSeriesSelection {
        channels: vec![0],
        derived: vec![],
    };

    buf.write_raw_csv(&mut csv, 10_000, 10_000, &selection)
        .unwrap();

    assert_eq!(
        String::from_utf8(csv).unwrap(),
        "timestamp_us,CH0\n10000,\n"
    );
}
