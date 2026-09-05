//! 后端测量引擎 — 权威缓冲 (金字塔快照) 上的统计/周期检测/自动设置
//!
//! 数据路径唯一: 一律经由 [`buffer_databuffer::DataBuffer::snapshot_window_budget`]
//! 取数 (窗口在 L0 内且原始点数 ≤ 4×budget → 原始快照; 否则金字塔自底向上选
//! 最小覆盖层) — 不新建任何降采样路径, 层级正确性由金字塔自身机制保证。
//!
//! 测量对象: 协议通道 + 派生序列 (MATH/Filter 等接入波形 sink 的派生输出) —
//! AutoSet 的时基由**所有被测序列中最慢的基波周期**驱动, 慢波形来自派生
//! 序列时同样参与检测 (快通道检测成功而慢派生失败会导致窗口过短的回归)。
//!
//! 层语义 (tier-aware):
//! - 层快照为交错 `(min, max)` 对 + 共享块末时间戳 (派生序列按层时间戳对齐
//!   采样, 每对两值); `dt` 一律从时间戳实测, 不按 16^(k+1) 假设;
//! - `vmin/vmax/vpp` 任何层都精确 (包络极值即真值); `vavg/vrms/duty` 在层上
//!   用对中点序列近似, 原始路径精确 — 载荷以 `from_tier/tier_level` 诚实标注;
//! - 周期检测带分辨率守卫: 仅接受 ≥ [`PERIOD_GUARD`]×dt 的周期, 粗层测不出
//!   快周期时返回 None (诚实拒绝, 不给混叠伪值); AutoSet 路径允许一次
//!   细化重查 (预算 ×4) 以扩展快周期量程。
//!
//! 所有计算函数均为同步纯计算 (锁内取快照、锁外算), 调用方须在
//! `tokio::task::block_in_place` / `spawn_blocking` 中执行, 不阻塞异步 worker。

use std::sync::Arc;

use buffer_databuffer::{
    DataBuffer, DerivedSeriesSelector, WaveformSampling, WaveformSeriesSelection,
};
use parking_lot::Mutex;
use serde::Serialize;

use dsp_measure::{channel_stats, detect_period, suggest_autoset, AutoSetSuggestion, ChannelFit};

/// 测量取数预算 — snapshot_window_budget 语义下最多取 4×budget 条目
/// (≈65k 条目 = 32k min/max 对或 65k 原始点; ACF 输入截断至 [`ACF_MAX_POINTS`])
const MEASURE_BUDGET: usize = 16_384;
/// AutoSet 细化重查预算 (×4 — 允许金字塔越一层, 扩展快周期量程)
const REFINE_BUDGET: usize = 65_536;
/// ACF 输入点数上限 — 超出截取最近 N 点 (等间隔无混叠, 仅缩短分析跨度)
const ACF_MAX_POINTS: usize = 131_072;
/// 周期分辨率守卫: 接受的周期必须 ≥ 6×采样间隔 (1-2-5 档位精度远低于此)
const PERIOD_GUARD: f64 = 6.0;
/// AutoSet 搜索窗口 (毫秒) = 最大时基档 5s × 10 div — 周期检测与回退拟合的范围
const AUTOSET_SEARCH_WINDOW_MS: f64 = 50_000.0;

/// 单通道测量结果 (频率/周期/占空比不可测为 null)
#[derive(Debug, Clone, Serialize)]
pub struct ChannelMeasurement {
    pub channel: usize,
    pub vpp: f64,
    pub vmin: f64,
    pub vmax: f64,
    pub vavg: f64,
    /// RMS (含直流)
    pub vrms: f64,
    /// RMS (去直流) — AC 耦合显示换算用
    pub vrms_ac: f64,
    pub duty: Option<f64>,
    pub freq: Option<f64>,
    pub period: Option<f64>,
}

/// 派生序列 (MATH/Filter 接入波形 sink) 测量结果 — 与通道同构统计 + 三元组键
#[derive(Debug, Clone, Serialize)]
pub struct DerivedMeasurement {
    pub sink_id: String,
    pub source_id: String,
    pub source_handle: String,
    pub vpp: f64,
    pub vmin: f64,
    pub vmax: f64,
    pub vavg: f64,
    pub vrms: f64,
    pub vrms_ac: f64,
    pub duty: Option<f64>,
    pub freq: Option<f64>,
    pub period: Option<f64>,
}

/// 单数据源测量快照 (JSON 推送; `from_tier` 诚实标注精度来源)
#[derive(Debug, Clone, Serialize)]
pub struct SourceMeasurements {
    pub source: String,
    /// 单调递增序号 — 前端按最新胜出
    pub seq: u64,
    pub window_ms: f64,
    pub latest_timestamp_us: u64,
    /// 快照来自金字塔层 (vavg/vrms 为包络中点近似)
    pub from_tier: bool,
    /// 金字塔层序号 (from_tier 时有效)
    pub tier_level: u8,
    pub channels: Vec<ChannelMeasurement>,
    pub derived: Vec<DerivedMeasurement>,
}

/// 被测序列标识 — 协议通道或派生序列三元组
#[derive(Debug, Clone)]
enum SeriesKey {
    Channel(usize),
    Derived {
        sink_id: String,
        source_id: String,
        source_handle: String,
    },
}

/// 快照窗口的序列视图 — 原始路径为逐样本; 层路径为交错 (min,max) 对
/// (派生序列按层时间戳对齐, 每对两值共享块末时间)
struct WindowSeries {
    key: SeriesKey,
    values: Vec<f32>,
}

struct ExtractedWindow {
    series: Vec<WindowSeries>,
    /// 相对最新时间戳 (毫秒, 升序); 层路径每对写入两次块末时间
    timestamps_ms: Vec<f64>,
    latest_us: u64,
    from_tier: bool,
    tier_level: u8,
    /// 数据实际时间跨度 (毫秒) — AutoSet 回退拟合用
    span_ms: f64,
}

impl ExtractedWindow {
    const fn is_tier(&self) -> bool {
        self.from_tier
    }

    /// dt (毫秒): 层路径为块对周期 (stride 2), 原始路径为样本间隔 (stride 1)
    fn dt_ms(&self) -> Option<f64> {
        let stride = if self.is_tier() { 2 } else { 1 };
        median_dt_ms(&self.timestamps_ms, stride)
    }
}

/// 经 snapshot_window_budget 提取窗口 (budget 决定原始/选层)。
///
/// 请求的通道号会被 clamp 到缓冲实际通道数; 通道与派生全空返回 None。
fn extract_window(
    buffer: &Mutex<DataBuffer>,
    window_ms: f64,
    channels: &[usize],
    derived: &[DerivedSeriesSelector],
    budget: usize,
) -> Option<ExtractedWindow> {
    let buf = buffer.lock();
    if buf.point_count() == 0 {
        return None;
    }
    let available = buf.channel_count();
    let mut selection = WaveformSeriesSelection::default();
    for &ch in channels {
        if ch < available {
            selection.channels.push(ch);
        }
    }
    selection.channels.sort_unstable();
    selection.channels.dedup();
    selection.derived = derived.to_vec();
    if selection.channels.is_empty() && selection.derived.is_empty() {
        return None;
    }
    let window = buf
        .snapshot_window_budget(-window_ms, 0.0, &selection, budget)
        .into_min_max(usize::MAX);
    drop(buf);

    let first_ts = window.timestamps.first().copied()?;
    let last_ts = window.timestamps.last().copied()?;
    let mut series = Vec::new();
    for &ch in &selection.channels {
        if let Some(values) = window.channels.get(ch).filter(|v| !v.is_empty()) {
            series.push(WindowSeries {
                key: SeriesKey::Channel(ch),
                values: values.clone(),
            });
        }
    }
    for sel in &selection.derived {
        let values = window
            .derived
            .get(&sel.sink_id)
            .and_then(|by_source| by_source.get(&sel.source_id))
            .and_then(|by_handle| by_handle.get(&sel.source_handle))
            .filter(|v| !v.is_empty());
        if let Some(values) = values {
            series.push(WindowSeries {
                key: SeriesKey::Derived {
                    sink_id: sel.sink_id.clone(),
                    source_id: sel.source_id.clone(),
                    source_handle: sel.source_handle.clone(),
                },
                values: values.clone(),
            });
        }
    }
    Some(ExtractedWindow {
        series,
        timestamps_ms: window.timestamps,
        latest_us: window.latest_timestamp_us,
        from_tier: window.sampling == WaveformSampling::MinMax,
        tier_level: window.buffer_tier.saturating_sub(1),
        span_ms: last_ts - first_ts,
    })
}

/// 包络极值 (跳过 NaN; 层路径极值即真值, 原始路径同样适用)
fn envelope_min(values: &[f32]) -> f64 {
    values.iter().fold(f64::INFINITY, |acc, &v| {
        if v.is_finite() {
            acc.min(f64::from(v))
        } else {
            acc
        }
    })
}

fn envelope_max(values: &[f32]) -> f64 {
    values.iter().fold(f64::NEG_INFINITY, |acc, &v| {
        if v.is_finite() {
            acc.max(f64::from(v))
        } else {
            acc
        }
    })
}

/// 相邻间隔中位数 (毫秒) — `stride` 为每组的时间戳下标步长
/// (原始 1; 层路径 2, 组内两值共享块末时间戳)。
/// 中位数抗个别缺口/块缺失, 不假设均匀分层。
fn median_dt_ms(timestamps: &[f64], stride: usize) -> Option<f64> {
    if timestamps.len() < stride * 2 {
        return None;
    }
    let mut diffs: Vec<f64> = timestamps
        .windows(stride + 1)
        .step_by(stride)
        .map(|w| w[stride] - w[0])
        .filter(|d| d.is_finite() && *d > 0.0)
        .collect();
    if diffs.is_empty() {
        return None;
    }
    diffs.sort_by(f64::total_cmp);
    Some(diffs[diffs.len() / 2])
}

/// 周期检测 + 分辨率守卫 — 中点序列 (层) 或原始序列上执行
fn guarded_period(values: &[f32], dt_sec: f64) -> Option<dsp_measure::PeriodEstimate> {
    let est = detect_period(values, dt_sec)?;
    (est.period_sec >= PERIOD_GUARD * dt_sec).then_some(est)
}

/// 截取最近 ACF_MAX_POINTS 点 (时间升序, 等间隔不变)
fn acf_input(values: &[f32]) -> &[f32] {
    if values.len() > ACF_MAX_POINTS {
        &values[values.len() - ACF_MAX_POINTS..]
    } else {
        values
    }
}

/// 解交错 min/max 对为中点序列 (仅层路径调用; 输入长度为偶数;
/// 派生序列每对两值相同, 中点即原值)
#[allow(clippy::cast_possible_truncation)] // 中点幅值远超 f32 下溢区间
fn midpoints(values: &[f32]) -> Vec<f32> {
    values
        .chunks_exact(2)
        .map(|pair| f64::midpoint(f64::from(pair[0]), f64::from(pair[1])) as f32)
        .collect()
}

/// 单序列统计 + 守卫周期 (通道与派生共用; 层/原始路径语义见模块文档)
struct SeriesStats {
    vpp: f64,
    vmin: f64,
    vmax: f64,
    vavg: f64,
    vrms: f64,
    vrms_ac: f64,
    duty: Option<f64>,
    period: Option<f64>,
}

fn measure_series(values: &[f32], is_tier: bool, dt_ms: Option<f64>) -> Option<SeriesStats> {
    if is_tier {
        let vmin = envelope_min(values);
        let vmax = envelope_max(values);
        if !vmin.is_finite() || !vmax.is_finite() {
            return None;
        }
        let mids = midpoints(values);
        let zero = dsp_measure::ChannelStats {
            vpp: 0.0,
            vmin: 0.0,
            vmax: 0.0,
            vavg: 0.0,
            vrms: 0.0,
            vrms_ac: 0.0,
            duty: None,
        };
        let stats = channel_stats(&mids).unwrap_or(zero);
        let period = dt_ms.and_then(|dt| guarded_period(acf_input(&mids), dt / 1000.0));
        Some(SeriesStats {
            vpp: vmax - vmin,
            vmin,
            vmax,
            vavg: stats.vavg,
            vrms: stats.vrms,
            vrms_ac: stats.vrms_ac,
            duty: stats.duty,
            period: period.map(|p| p.period_sec),
        })
    } else {
        let stats = channel_stats(values)?;
        let period = dt_ms.and_then(|dt| guarded_period(acf_input(values), dt / 1000.0));
        Some(SeriesStats {
            vpp: stats.vpp,
            vmin: stats.vmin,
            vmax: stats.vmax,
            vavg: stats.vavg,
            vrms: stats.vrms,
            vrms_ac: stats.vrms_ac,
            duty: stats.duty,
            period: period.map(|p| p.period_sec),
        })
    }
}

/// 计算单数据源测量快照 (同步; 调用方负责放入阻塞线程)。
///
/// `channels` 为空表示全部协议通道; `derived` 为要测量的派生序列三元组。
pub fn compute_source_measurements(
    buffer: &Arc<Mutex<DataBuffer>>,
    source: &str,
    window_ms: f64,
    derived: &[DerivedSeriesSelector],
    seq: u64,
) -> Option<SourceMeasurements> {
    if !window_ms.is_finite() || window_ms <= 0.0 {
        return None;
    }
    // 全通道: 请求 0..=255 足够 (实际通道数由 extract clamp)
    let all_channels: Vec<usize> = (0..256).collect();
    let extracted = extract_window(buffer, window_ms, &all_channels, derived, MEASURE_BUDGET)?;
    let dt_ms = extracted.dt_ms();
    let is_tier = extracted.is_tier();

    let mut channels_out = Vec::new();
    let mut derived_out = Vec::new();
    for s in &extracted.series {
        let Some(stats) = measure_series(&s.values, is_tier, dt_ms) else {
            continue;
        };
        match &s.key {
            SeriesKey::Channel(channel) => channels_out.push(ChannelMeasurement {
                channel: *channel,
                vpp: stats.vpp,
                vmin: stats.vmin,
                vmax: stats.vmax,
                vavg: stats.vavg,
                vrms: stats.vrms,
                vrms_ac: stats.vrms_ac,
                duty: stats.duty,
                freq: stats.period.map(|p| 1.0 / p),
                period: stats.period,
            }),
            SeriesKey::Derived {
                sink_id,
                source_id,
                source_handle,
            } => derived_out.push(DerivedMeasurement {
                sink_id: sink_id.clone(),
                source_id: source_id.clone(),
                source_handle: source_handle.clone(),
                vpp: stats.vpp,
                vmin: stats.vmin,
                vmax: stats.vmax,
                vavg: stats.vavg,
                vrms: stats.vrms,
                vrms_ac: stats.vrms_ac,
                duty: stats.duty,
                freq: stats.period.map(|p| 1.0 / p),
                period: stats.period,
            }),
        }
    }
    if channels_out.is_empty() && derived_out.is_empty() {
        return None;
    }
    Some(SourceMeasurements {
        source: source.to_string(),
        seq,
        window_ms,
        latest_timestamp_us: extracted.latest_us,
        from_tier: extracted.from_tier,
        tier_level: extracted.tier_level,
        channels: channels_out,
        derived: derived_out,
    })
}

/// 计算自动设置建议 (同步; 调用方负责放入阻塞线程)。
///
/// 搜索最近 [`AUTOSET_SEARCH_WINDOW_MS`] 窗口: 周期可测 → 显示
/// [`dsp_measure::PERIODS_SHOWN`] 个**最慢基波周期** (协议通道与派生序列
/// 一起参与取最大); 全部不可测 → 回退快照实际数据跨度拟合。
/// 守卫不满足 (粗层测不到快周期) 时细化重查一次 (预算 ×4)。
pub fn compute_autoset_suggestion(
    buffer: &Arc<Mutex<DataBuffer>>,
    channels: &[usize],
    derived: &[DerivedSeriesSelector],
    shared_y: bool,
    current_v_per_div: &[f64],
) -> Option<AutoSetSuggestion> {
    let requested: Vec<usize> = if channels.is_empty() {
        (0..256).collect()
    } else {
        channels.to_vec()
    };
    let extracted = extract_window(
        buffer,
        AUTOSET_SEARCH_WINDOW_MS,
        &requested,
        derived,
        MEASURE_BUDGET,
    )?;
    let mut fits = build_fits(&extracted);
    // 细化重查: 有序列处于层路径且周期被守卫拒绝 → 预算 ×4 越一层重测
    if extracted.is_tier() && fits.iter().any(|fit| fit.period_sec.is_none()) {
        if let Some(refined) = extract_window(
            buffer,
            AUTOSET_SEARCH_WINDOW_MS,
            &requested,
            derived,
            REFINE_BUDGET,
        ) {
            let refined_fits = build_fits(&refined);
            for (fit, better) in fits.iter_mut().zip(refined_fits) {
                if fit.period_sec.is_none() {
                    fit.period_sec = better.period_sec;
                }
            }
        }
    }
    if fits.is_empty() {
        return None;
    }
    // 平直信号保持现值: 通道按请求下标取, 派生序列无现值传 0 (触发默认 1)
    let mut current = Vec::with_capacity(fits.len());
    let mut derived_seen = 0_usize;
    for s in &extracted.series {
        match &s.key {
            SeriesKey::Channel(ch) => {
                current.push(current_v_per_div.get(*ch).copied().unwrap_or(1.0));
            }
            SeriesKey::Derived { .. } => {
                derived_seen += 1;
                current.push(0.0);
            }
        }
    }
    let _ = derived_seen;
    let fallback_window_sec = (extracted.span_ms / 1000.0).max(0.0);
    Some(suggest_autoset(
        &fits,
        shared_y,
        &current,
        fallback_window_sec,
    ))
}

/// 从提取窗口组装每序列拟合输入 (包络极值 + 守卫周期), 顺序与 series 一致
fn build_fits(extracted: &ExtractedWindow) -> Vec<ChannelFit> {
    let dt_ms = extracted.dt_ms();
    let is_tier = extracted.is_tier();
    extracted
        .series
        .iter()
        .filter_map(|s| {
            let stats = measure_series(&s.values, is_tier, dt_ms)?;
            Some(ChannelFit {
                vmin: stats.vmin,
                vmax: stats.vmax,
                vpp: stats.vpp,
                period_sec: stats.period,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_lossless,
        clippy::cast_sign_loss
    )]
    use super::*;
    use crate::AppState;

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
        let s =
            compute_autoset_suggestion(&buf, &[], &derived_sel(), false, &[1.0]).expect("有建议");
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
        let m =
            compute_source_measurements(&buf, "src", 6_000.0, &derived_sel(), 3).expect("有测量");
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
}
