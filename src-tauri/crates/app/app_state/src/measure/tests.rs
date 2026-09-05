//! 测量引擎测试 — AppState + DataBuffer pub API 驱动

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::cast_sign_loss
)]
use super::*;
use crate::AppState;
use buffer_databuffer::DerivedSeriesSelector;

/// 指定频率/时长的正弦 @ 10kHz 采样
fn push_sine(state: &AppState, source: &str, freq: f64, seconds: f64) {
    let buf = state.data_plane.buffer_for(source);
    let mut b = buf.lock();
    let rate = 10_000.0_f64;
    let n = (seconds * rate) as usize;
    for i in 0..n {
        let t = i as f64 / rate;
        let v = (std::f64::consts::TAU * freq * t).sin() as f32;
        b.push_frame_at((t * 1e6) as u64, &[v]);
    }
}

/// 向缓冲追加派生序列 (sink/source/handle 三元组) — 指定频率正弦 @ 同采样率
fn push_derived_sine(
    state: &AppState,
    source: &str,
    sink: &str,
    math: &str,
    freq: f64,
    seconds: f64,
) {
    let buf = state.data_plane.buffer_for(source);
    let b = buf.lock();
    let rate = 10_000.0_f64;
    let n = (seconds * rate) as usize;
    let idx = b.derived_port_index_of(sink, math, "value");
    for i in 0..n {
        let t = i as f64 / rate;
        let v = (std::f64::consts::TAU * freq * t).sin() as f32;
        b.push_derived_ts_idx(idx, (t * 1e6) as u64, v);
    }
}

fn derived_sel() -> Vec<DerivedSeriesSelector> {
    vec![DerivedSeriesSelector {
        sink_id: "w1".to_string(),
        source_id: "math1".to_string(),
        source_handle: "value".to_string(),
    }]
}

#[test]
fn raw_path_measures_sine_exactly() {
    let state = AppState::new();
    push_sine(&state, "src", 1_000.0, 3.0);
    let buf = state.data_plane.buffer_for("src");
    let m = compute_source_measurements(&buf, "src", 3_000.0, &[], 1).expect("有测量");
    assert!(!m.from_tier, "3s @ 10kHz = 30k 点应走原始路径");
    let ch = &m.channels[0];
    // 10 样本/周期的采样栅格: 峰值落在样本间 → vpp = 2·sin(72°) ≈ 1.902 (正确采样值)
    assert!((ch.vpp - 1.9021).abs() < 1e-3, "vpp={}", ch.vpp);
    assert!(ch.vavg.abs() < 1e-6);
    assert!((ch.vrms - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-4);
    assert!(
        ch.period.is_some_and(|p| (p - 1e-3).abs() < 1e-6),
        "period={:?}",
        ch.period
    );
    assert!(
        ch.freq.is_some_and(|f| (f - 1_000.0).abs() < 1e-3),
        "freq={:?}",
        ch.freq
    );
    assert!(ch.duty.is_some_and(|d| (d - 0.5).abs() < 1e-3));
}

#[test]
fn tier_path_flags_envelope_and_still_measures() {
    let state = AppState::new();
    // 5Hz 正弦 @ 10kHz, 12s = 120k 点 > 4×budget → 金字塔层;
    // tier1 容量 4096 对 (~6.5s) 覆盖不住 12s → tier2 服务, dt_pair≈25.6ms,
    // 守卫下限 ~154ms — 200ms 周期放行 (中点序列 7.8 样本/周期)
    push_sine(&state, "src", 5.0, 12.0);
    let buf = state.data_plane.buffer_for("src");
    let m = compute_source_measurements(&buf, "src", 12_000.0, &[], 2).expect("有测量");
    assert!(m.from_tier, "12s @ 10kHz 应走层路径");
    let ch = &m.channels[0];
    // 包络极值在任何层都精确
    assert!((ch.vpp - 2.0).abs() < 1e-2, "层路径 vpp={}", ch.vpp);
    assert!(
        ch.period.is_some_and(|p| (p - 0.2).abs() < 0.02),
        "层路径 period={:?}",
        ch.period
    );
}

#[test]
fn autoset_suggests_two_periods_of_1khz() {
    let state = AppState::new();
    push_sine(&state, "src", 1_000.0, 3.0);
    let buf = state.data_plane.buffer_for("src");
    let s = compute_autoset_suggestion(&buf, &[], &[], false, &[1.0]).expect("有建议");
    // 1ms 周期 × 2 = 2ms 窗口 → 0.2ms/div (表内直取)
    assert!(
        (s.time_base_sec - 2e-4).abs() < 1e-12,
        "tb={}",
        s.time_base_sec
    );
    assert!(!s.clamped);
    // vpp 2 → 2/(8×0.7)=0.357 → 0.5 V/div
    assert!((s.channels[0].v_per_div - 0.5).abs() < 1e-12);
    assert!((s.channels[0].position.abs()) < 1e-6);
}

/// 回归: 慢波形来自派生序列时, AutoSet 时基必须由它驱动 —
/// 快通道 (1kHz) 检测成功而慢派生 (0.5Hz) 不参与会导致窗口过短
#[test]
fn autoset_slow_derived_series_drives_time_base() {
    let state = AppState::new();
    push_sine(&state, "src", 1_000.0, 6.0);
    push_derived_sine(&state, "src", "w1", "math1", 0.5, 6.0);
    let buf = state.data_plane.buffer_for("src");
    let s = compute_autoset_suggestion(&buf, &[], &derived_sel(), false, &[1.0]).expect("有建议");
    // 最慢周期 = 派生 2s → 窗口 2×2s = 4s → 0.4s/div → 向上取 0.5s/div
    assert!(
        (s.time_base_sec - 0.5).abs() < 1e-12,
        "tb={}",
        s.time_base_sec
    );
    assert!(!s.clamped);
}

#[test]
fn measurements_include_derived_series() {
    let state = AppState::new();
    push_sine(&state, "src", 1_000.0, 6.0);
    push_derived_sine(&state, "src", "w1", "math1", 0.5, 6.0);
    let buf = state.data_plane.buffer_for("src");
    let m = compute_source_measurements(&buf, "src", 6_000.0, &derived_sel(), 3).expect("有测量");
    assert_eq!(m.channels.len(), 1);
    assert_eq!(m.derived.len(), 1);
    let d = &m.derived[0];
    assert_eq!(d.sink_id, "w1");
    assert_eq!(d.source_id, "math1");
    assert!(
        d.period.is_some_and(|p| (p - 2.0).abs() < 0.02),
        "derived period={:?}",
        d.period
    );
    assert!(d.freq.is_some_and(|f| (f - 0.5).abs() < 0.01));
    assert!((d.vpp - 2.0).abs() < 1e-2);
}

#[test]
fn autoset_without_data_returns_none() {
    let state = AppState::new();
    let buf = state.data_plane.buffer_for("empty");
    assert!(compute_autoset_suggestion(&buf, &[], &[], false, &[1.0]).is_none());
    assert!(compute_source_measurements(&buf, "empty", 1_000.0, &[], 0).is_none());
}

#[test]
fn autoset_flat_signal_falls_back_to_span() {
    let state = AppState::new();
    let buf = state.data_plane.buffer_for("flat");
    {
        let mut b = buf.lock();
        for i in 0..5_000 {
            let t = i as f64 / 10_000.0;
            b.push_frame_at((t * 1e6) as u64, &[2.5_f32]);
        }
    }
    let s = compute_autoset_suggestion(&buf, &[], &[], false, &[0.05]).expect("有建议");
    // 平直: 周期不可测 → 回退数据跨度 0.5s → 目标时基 0.05s/div (表内直取)
    assert!(
        (s.time_base_sec - 0.05).abs() < 1e-12,
        "tb={}",
        s.time_base_sec
    );
    // 平直信号保持现值 0.05
    assert!((s.channels[0].v_per_div - 0.05).abs() < 1e-12);
    assert!((s.channels[0].position - 2.5).abs() < 1e-6);
}

#[test]
fn out_of_range_channel_request_clamps_to_none() {
    let state = AppState::new();
    push_sine(&state, "src", 1_000.0, 3.0);
    let buf = state.data_plane.buffer_for("src");
    // 仅请求超出缓冲通道范围的通道 → clamp 后无有效通道 → None
    assert!(
        compute_autoset_suggestion(&buf, &[999], &[], false, &[1.0]).is_none(),
        "越界通道请求应返回 None"
    );
}
