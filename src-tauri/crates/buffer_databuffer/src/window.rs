//! `DataBuffer` 窗口切片 — WaveformWindow 查询 (get_window / get_recent) 与 NaN 对齐
//!
//! 派生缓冲区创建较晚时, 窗口早期位置填 NaN (表示 "尚无数据"),
//! 保证 derived[i] 与 channels[ch][i] 严格按时间戳对齐。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::DataBuffer;

/// 波形数据窗口 — 供前端查询
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformWindow {
    /// 组级单调序号 — 分片并发推送时前端按 "最新 seq 胜出" 丢弃旧快照
    #[serde(default)]
    pub seq: u64,
    /// 时间戳数组 (相对最新的偏移, 单位: 毫秒)
    pub timestamps: Vec<i64>,
    /// 每通道的数据数组
    pub channels: Vec<Vec<f32>>,
    /// 当前检测到的通道数
    pub channel_count: usize,
    /// 派生通道数据 (Math/Filter 等节点的输出, 作为 Waveform sink 的输入)
    /// key1 = sink_widget_id, key2 = source_widget_id, value = 与 timestamps 对齐的数据
    #[serde(default)]
    pub derived: HashMap<String, HashMap<String, Vec<f32>>>,
    /// 后端波形缓冲区当前点数 (用于状态栏显示缓存使用率)
    #[serde(default)]
    pub buffer_points: usize,
    /// 后端波形缓冲区最大容量 (点)
    #[serde(default)]
    pub buffer_capacity: usize,
}

impl DataBuffer {
    /// 切片所有派生缓冲区 — 用于 get_window (按 start_idx..end_idx 索引)
    ///
    /// 返回 HashMap<sink_id, HashMap<source_id, Vec<f32>>>,
    /// 每个 Vec<f32> 长度 = end_idx - start_idx, 与 window timestamps 对齐。
    /// 派生缓冲区创建较晚时, 早期位置填 NaN (表示 "尚无数据")。
    fn slice_all_derived_window(
        &self,
        start_idx: usize,
        end_idx: usize,
        total_ts: usize,
    ) -> HashMap<String, HashMap<String, Vec<f32>>> {
        let window_len = end_idx - start_idx;
        let mut result: HashMap<String, HashMap<String, Vec<f32>>> = HashMap::new();
        for e in &self.derived_list {
            // 跳过空条目 (批首注册可能尚无数据) — 保持旧语义: 只输出有过数据的键
            let rb = &e.rb;
            if rb.len() == 0 {
                continue;
            }
            let m = rb.len();
            // derived[0] 对应 timestamps[offset] (offset = total_ts - m)
            let offset = total_ts.saturating_sub(m);
            let all_data = rb.all();
            let mut v = Vec::with_capacity(window_len);
            for i in start_idx..end_idx {
                if i < offset {
                    v.push(f32::NAN); // 派生缓冲区创建之前 → NaN
                } else {
                    let di = i - offset;
                    if di < m {
                        v.push(all_data[di]);
                    } else {
                        v.push(f32::NAN);
                    }
                }
            }
            result
                .entry(e.sink.clone())
                .or_default()
                .insert(e.source.clone(), v);
        }
        result
    }

    /// 切片所有派生缓冲区 — 用于 get_recent (取最近 count 个点)
    ///
    /// 每个 Vec<f32> 长度 = count, 与 recent timestamps 对齐。
    /// 派生缓冲区不足 count 时, 开头填 NaN。
    fn slice_all_derived_recent(&self, count: usize) -> HashMap<String, HashMap<String, Vec<f32>>> {
        let mut result: HashMap<String, HashMap<String, Vec<f32>>> = HashMap::new();
        for e in &self.derived_list {
            // 跳过空条目 (批首注册可能尚无数据) — 保持旧语义: 只输出有过数据的键
            if e.rb.len() == 0 {
                continue;
            }
            let data = e.rb.recent(count);
            if data.len() < count {
                // 开头补 NaN (派生缓冲区创建较晚)
                let pad = count - data.len();
                let mut v = vec![f32::NAN; pad];
                v.extend_from_slice(&data);
                result
                    .entry(e.sink.clone())
                    .or_default()
                    .insert(e.source.clone(), v);
            } else {
                result
                    .entry(e.sink.clone())
                    .or_default()
                    .insert(e.source.clone(), data);
            }
        }
        result
    }

    /// 获取时间窗口内的数据
    /// start_ms / end_ms 为相对最新时间戳的偏移 (毫秒, 负数=过去)
    pub fn get_window(&self, start_ms: i64, end_ms: i64) -> WaveformWindow {
        let all_ts = self.timestamps.all();
        if all_ts.is_empty() {
            return WaveformWindow {
                seq: 0,
                timestamps: vec![],
                channels: vec![],
                channel_count: self.num_channels,
                derived: HashMap::new(),
                buffer_points: 0,
                buffer_capacity: self.max_points,
            };
        }

        let latest_us = all_ts[all_ts.len() - 1];

        let start_us = ((latest_us as i64) + start_ms * 1000).max(0) as u64;
        let end_us = ((latest_us as i64) + end_ms * 1000).max(0) as u64;

        // 找到范围内的索引
        let mut start_idx = 0;
        let mut end_idx = all_ts.len();
        for (i, &ts) in all_ts.iter().enumerate() {
            if ts >= start_us {
                start_idx = i;
                break;
            }
        }
        for (i, &ts) in all_ts.iter().enumerate().skip(start_idx) {
            if ts > end_us {
                end_idx = i;
                break;
            }
        }

        let window_ts: Vec<i64> = all_ts[start_idx..end_idx]
            .iter()
            .map(|&ts| (ts as i64 - latest_us as i64) / 1000)
            .collect();

        let window_channels: Vec<Vec<f32>> = (0..self.num_channels)
            .map(|ch| self.channels[ch].recent(self.timestamps.len())[start_idx..end_idx].to_vec())
            .collect();

        let derived = self.slice_all_derived_window(start_idx, end_idx, all_ts.len());

        WaveformWindow {
            seq: 0,
            timestamps: window_ts,
            channels: window_channels,
            channel_count: self.num_channels,
            derived,
            buffer_points: self.timestamps.len(),
            buffer_capacity: self.max_points,
        }
    }

    /// 获取最近 N 个点
    pub fn get_recent(&self, count: usize) -> WaveformWindow {
        let ts = self.timestamps.recent(count);
        let latest_us = self.timestamps.all().last().copied().unwrap_or(0);

        let rel_ts: Vec<i64> = ts
            .iter()
            .map(|&t| (t as i64 - latest_us as i64) / 1000)
            .collect();

        let channels: Vec<Vec<f32>> = (0..self.num_channels)
            .map(|ch| self.channels[ch].recent(count))
            .collect();

        let derived = self.slice_all_derived_recent(count);

        WaveformWindow {
            seq: 0,
            timestamps: rel_ts,
            channels,
            channel_count: self.num_channels,
            derived,
            buffer_points: self.timestamps.len(),
            buffer_capacity: self.max_points,
        }
    }
}