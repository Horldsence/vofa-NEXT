//! # vofa-next-buffer
//!
//! 数据缓冲区与节点图路由。
//!
//! - [`RingBuffer`][]: 泛型环形缓冲区
//! - [`DataBuffer`][]: 多通道时间序列缓冲区
//! - [`NodeGraph`]: 节点连接关系管理 + 数据路由

pub mod graph;

pub use graph::{Edge, NodeGraph, RoutedData};

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use vofa_next_core::DataFrame;

/// 泛型环形缓冲区 — 固定容量, 覆盖最旧数据
#[derive(Debug, Clone)]
pub struct RingBuffer<T: Clone + Default> {
    buf: Vec<T>,
    head: usize,
    len: usize,
    capacity: usize,
}

impl<T: Clone + Default> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            buf: vec![T::default(); cap],
            head: 0,
            len: 0,
            capacity: cap,
        }
    }

    /// 追加一个元素, 若已满则覆盖最旧数据
    pub fn push(&mut self, value: T) {
        self.buf[self.head] = value;
        self.head = (self.head + 1) % self.capacity;
        if self.len < self.capacity {
            self.len += 1;
        }
    }

    /// 追加多个元素
    pub fn extend(&mut self, values: &[T]) {
        for v in values {
            self.push(v.clone());
        }
    }

    /// 获取最近 n 个元素 (按时间顺序, 最旧→最新)
    pub fn recent(&self, n: usize) -> Vec<T> {
        let count = n.min(self.len);
        if count == 0 {
            return Vec::new();
        }
        let start = (self.head + self.capacity - count) % self.capacity;
        let mut result = Vec::with_capacity(count);
        for i in 0..count {
            result.push(self.buf[(start + i) % self.capacity].clone());
        }
        result
    }

    /// 获取全部数据 (按时间顺序)
    pub fn all(&self) -> Vec<T> {
        self.recent(self.len)
    }

    /// 当前元素数量
    pub fn len(&self) -> usize {
        self.len
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 容量
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 清空
    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    /// 修改容量 (保留已有数据, 超出部分截断)
    pub fn resize(&mut self, new_capacity: usize) {
        let cap = new_capacity.max(1);
        let existing = self.all();
        self.buf = vec![T::default(); cap];
        self.head = 0;
        self.len = 0;
        self.capacity = cap;
        let start = if existing.len() > cap {
            existing.len() - cap
        } else {
            0
        };
        for v in &existing[start..] {
            self.push(v.clone());
        }
    }
}

/// 波形数据窗口 — 供前端查询
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformWindow {
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

/// 原始数据块
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDataChunk {
    pub timestamp_us: u64,
    pub bytes: Vec<u8>,
}

/// 原始数据批次
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDataBatch {
    pub chunks: Vec<RawDataChunk>,
    pub total_bytes: u64,
    pub dropped_bytes: u64,
}

/// 原始数据收集器 — 固定容量, 超过时丢弃最旧块
#[derive(Debug, Clone)]
pub struct RawDataCollector {
    chunks: VecDeque<RawDataChunk>,
    capacity: usize,
    total_bytes: u64,
    dropped_bytes: u64,
}

impl RawDataCollector {
    /// 默认容量: 1 MiB
    pub const DEFAULT_CAPACITY: usize = 1_048_576;

    /// 使用默认容量创建
    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    /// 使用指定容量创建
    pub fn with_capacity(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            chunks: VecDeque::new(),
            capacity: cap,
            total_bytes: 0,
            dropped_bytes: 0,
        }
    }

    /// 推入一块原始数据; 若超出容量则丢弃最旧块
    pub fn push_chunk(&mut self, timestamp_us: u64, bytes: &[u8]) {
        self.total_bytes += bytes.len() as u64;
        self.chunks.push_back(RawDataChunk {
            timestamp_us,
            bytes: bytes.to_vec(),
        });

        while self.stored_bytes() > self.capacity && !self.chunks.is_empty() {
            if let Some(front) = self.chunks.pop_front() {
                self.dropped_bytes += front.bytes.len() as u64;
            }
        }
    }

    /// 取出不超过 max_bytes 的若干完整块
    pub fn drain_batch(&mut self, max_bytes: usize) -> RawDataBatch {
        let mut drained = Vec::new();
        let mut acc = 0usize;
        while let Some(front) = self.chunks.front() {
            let next = acc.saturating_add(front.bytes.len());
            if next > max_bytes && !drained.is_empty() {
                break;
            }
            if let Some(chunk) = self.chunks.pop_front() {
                acc = acc.saturating_add(chunk.bytes.len());
                drained.push(chunk);
            }
        }
        RawDataBatch {
            chunks: drained,
            total_bytes: self.total_bytes,
            dropped_bytes: self.dropped_bytes,
        }
    }

    /// 清空所有块并重置计数器
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.total_bytes = 0;
        self.dropped_bytes = 0;
    }

    /// 设置容量 (保留最近块)
    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
        while self.stored_bytes() > self.capacity && !self.chunks.is_empty() {
            if let Some(front) = self.chunks.pop_front() {
                self.dropped_bytes += front.bytes.len() as u64;
            }
        }
    }

    /// 累计写入字节数 (含已丢弃)
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// 累计丢弃字节数
    pub fn dropped_bytes(&self) -> u64 {
        self.dropped_bytes
    }

    fn stored_bytes(&self) -> usize {
        self.chunks.iter().map(|c| c.bytes.len()).sum()
    }
}

impl Default for RawDataCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// 多通道时间序列数据缓冲区
pub struct DataBuffer {
    /// 每通道一个环形缓冲区
    channels: Vec<RingBuffer<f32>>,
    /// 时间戳缓冲区 (微秒)
    timestamps: RingBuffer<u64>,
    /// 最大点数
    max_points: usize,
    /// 当前通道数 (可动态变化)
    num_channels: usize,
    /// 派生数据缓冲区: (sink_widget_id, source_widget_id) → 环形缓冲区
    /// 与 timestamps 同步 push, 保证时间戳完全对齐
    derived_buffers: HashMap<(String, String), RingBuffer<f32>>,
}

impl DataBuffer {
    pub fn new(max_points: usize, num_channels: usize) -> Self {
        let nc = num_channels.max(1);
        Self {
            channels: (0..nc).map(|_| RingBuffer::new(max_points)).collect(),
            timestamps: RingBuffer::new(max_points),
            max_points,
            num_channels: nc,
            derived_buffers: HashMap::new(),
        }
    }

    /// 推入一帧数据
    pub fn push_frame(&mut self, frame: &DataFrame) {
        // 动态调整通道数
        let frame_ch = frame.channels.len();
        if frame_ch > self.num_channels {
            self.resize_channels(frame_ch);
        }
        self.timestamps.push(frame.timestamp);
        for i in 0..self.num_channels {
            let val = if i < frame.channels.len() {
                frame.channels[i]
            } else {
                0.0
            };
            self.channels[i].push(val);
        }
    }

    /// 推入派生数据 (与最近一次 push_frame 的时间戳对齐)
    ///
    /// 在 data_loop 中, 每帧 evaluate_all_graphs_with 后调用:
    /// 遍历 graph.edges, 对每条 edge, 若 source 在 output_snapshot 中,
    /// 调用本方法将值 push 到 (sink_id, source_id) 的环形缓冲区。
    ///
    /// **时间对齐**: 派生缓冲区与 timestamps 共享同一时间轴,
    /// 保证 derived[i] 与 channels[ch][i] 对应同一帧。
    pub fn push_derived(&mut self, sink_id: &str, source_id: &str, value: f32) {
        let key = (sink_id.to_string(), source_id.to_string());
        self.derived_buffers
            .entry(key)
            .or_insert_with(|| RingBuffer::new(self.max_points))
            .push(value);
    }

    /// 清空所有派生缓冲区 (断开连接/清数据时调用)
    pub fn clear_derived(&mut self) {
        self.derived_buffers.clear();
    }

    /// 移除指定 sink 的派生缓冲区 (widget 删除时调用)
    pub fn remove_derived_sink(&mut self, sink_id: &str) {
        self.derived_buffers.retain(|(s, _), _| s != sink_id);
    }

    /// 调整通道数 (仅增大, 保留已有数据)
    fn resize_channels(&mut self, new_count: usize) {
        while self.channels.len() < new_count {
            self.channels.push(RingBuffer::new(self.max_points));
        }
        self.num_channels = new_count;
    }

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
        for ((sink, source), rb) in &self.derived_buffers {
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
                .entry(sink.clone())
                .or_default()
                .insert(source.clone(), v);
        }
        result
    }

    /// 切片所有派生缓冲区 — 用于 get_recent (取最近 count 个点)
    ///
    /// 每个 Vec<f32> 长度 = count, 与 recent timestamps 对齐。
    /// 派生缓冲区不足 count 时, 开头填 NaN。
    fn slice_all_derived_recent(&self, count: usize) -> HashMap<String, HashMap<String, Vec<f32>>> {
        let mut result: HashMap<String, HashMap<String, Vec<f32>>> = HashMap::new();
        for ((sink, source), rb) in &self.derived_buffers {
            let data = rb.recent(count);
            if data.len() < count {
                // 开头补 NaN (派生缓冲区创建较晚)
                let pad = count - data.len();
                let mut v = vec![f32::NAN; pad];
                v.extend_from_slice(&data);
                result
                    .entry(sink.clone())
                    .or_default()
                    .insert(source.clone(), v);
            } else {
                result
                    .entry(sink.clone())
                    .or_default()
                    .insert(source.clone(), data);
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
            timestamps: rel_ts,
            channels,
            channel_count: self.num_channels,
            derived,
            buffer_points: self.timestamps.len(),
            buffer_capacity: self.max_points,
        }
    }

    /// 获取单通道最近 N 个点
    pub fn get_channel(&self, ch: usize, count: usize) -> Vec<f32> {
        if ch >= self.channels.len() {
            return Vec::new();
        }
        self.channels[ch].recent(count)
    }

    /// 当前通道数
    pub fn channel_count(&self) -> usize {
        self.num_channels
    }

    /// 当前点数
    pub fn point_count(&self) -> usize {
        self.timestamps.len()
    }

    /// 最大容量 (点)
    pub fn max_points(&self) -> usize {
        self.max_points
    }

    /// 设置最大容量 (保留最近数据)
    pub fn set_max_points(&mut self, max_points: usize) {
        let new_max = max_points.max(1);
        if new_max == self.max_points {
            return;
        }
        self.max_points = new_max;
        self.timestamps.resize(new_max);
        for ch in &mut self.channels {
            ch.resize(new_max);
        }
        for buf in self.derived_buffers.values_mut() {
            buf.resize(new_max);
        }
    }

    /// 清空
    pub fn clear(&mut self) {
        for ch in &mut self.channels {
            ch.clear();
        }
        self.timestamps.clear();
        self.derived_buffers.clear();
    }

    /// 设置通道数 (清空已有数据)
    pub fn set_channels(&mut self, count: usize) {
        let nc = count.max(1);
        self.channels = (0..nc).map(|_| RingBuffer::new(self.max_points)).collect();
        self.timestamps.clear();
        self.num_channels = nc;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ringbuffer_push_and_recent() {
        let mut rb: RingBuffer<i32> = RingBuffer::new(5);
        rb.push(1);
        rb.push(2);
        rb.push(3);
        assert_eq!(rb.recent(2), vec![2, 3]);
        assert_eq!(rb.recent(10), vec![1, 2, 3]);
        assert_eq!(rb.len(), 3);
    }

    #[test]
    fn test_ringbuffer_overflow() {
        let mut rb: RingBuffer<i32> = RingBuffer::new(3);
        rb.push(1);
        rb.push(2);
        rb.push(3);
        rb.push(4);
        rb.push(5);
        assert_eq!(rb.len(), 3);
        assert_eq!(rb.all(), vec![3, 4, 5]);
    }

    #[test]
    fn test_ringbuffer_extend() {
        let mut rb: RingBuffer<i32> = RingBuffer::new(5);
        rb.extend(&[1, 2, 3, 4, 5]);
        assert_eq!(rb.all(), vec![1, 2, 3, 4, 5]);
        rb.extend(&[6, 7]);
        assert_eq!(rb.all(), vec![3, 4, 5, 6, 7]);
    }

    #[test]
    fn test_ringbuffer_empty() {
        let rb: RingBuffer<i32> = RingBuffer::new(5);
        assert!(rb.is_empty());
        assert_eq!(rb.recent(10), Vec::<i32>::new());
    }

    #[test]
    fn test_ringbuffer_clear() {
        let mut rb: RingBuffer<i32> = RingBuffer::new(5);
        rb.push(1);
        rb.push(2);
        rb.clear();
        assert!(rb.is_empty());
    }

    #[test]
    fn test_ringbuffer_resize_smaller() {
        let mut rb: RingBuffer<i32> = RingBuffer::new(5);
        rb.extend(&[1, 2, 3, 4, 5]);
        rb.resize(3);
        assert_eq!(rb.capacity(), 3);
        assert_eq!(rb.all(), vec![3, 4, 5]);
    }

    #[test]
    fn test_ringbuffer_resize_larger() {
        let mut rb: RingBuffer<i32> = RingBuffer::new(3);
        rb.extend(&[1, 2, 3]);
        rb.resize(5);
        assert_eq!(rb.capacity(), 5);
        assert_eq!(rb.all(), vec![1, 2, 3]);
    }

    #[test]
    fn test_ringbuffer_capacity_one() {
        let mut rb: RingBuffer<i32> = RingBuffer::new(1);
        rb.push(1);
        rb.push(2);
        rb.push(3);
        assert_eq!(rb.len(), 1);
        assert_eq!(rb.all(), vec![3]);
    }

    // ===== DataBuffer tests =====

    #[test]
    fn test_databuffer_push_and_get_recent() {
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
    fn test_databuffer_empty() {
        let buf = DataBuffer::new(100, 4);
        let w = buf.get_recent(10);
        assert_eq!(w.channel_count, 4);
        assert!(w.timestamps.is_empty());
        assert!(w.channels.is_empty() || w.channels.iter().all(|c| c.is_empty()));
    }

    #[test]
    fn test_databuffer_clear() {
        let mut buf = DataBuffer::new(100, 2);
        buf.push_frame(&DataFrame::new(vec![1.0, 2.0]));
        buf.clear();
        assert_eq!(buf.point_count(), 0);
    }

    #[test]
    fn test_databuffer_auto_expand_channels() {
        let mut buf = DataBuffer::new(100, 2);
        buf.push_frame(&DataFrame::new(vec![1.0, 2.0]));
        assert_eq!(buf.channel_count(), 2);
        // 接收到更多通道的帧 → 自动扩展
        buf.push_frame(&DataFrame::new(vec![1.0, 2.0, 3.0, 4.0]));
        assert_eq!(buf.channel_count(), 4);
        // 新通道只有第二帧的数据 (第一帧时还不存在)
        let w = buf.get_recent(2);
        assert_eq!(w.channels[0], vec![1.0, 1.0]);
        assert_eq!(w.channels[3], vec![4.0]);
    }

    #[test]
    fn test_databuffer_set_channels() {
        let mut buf = DataBuffer::new(100, 2);
        buf.push_frame(&DataFrame::new(vec![1.0, 2.0]));
        buf.set_channels(4);
        assert_eq!(buf.channel_count(), 4);
        assert_eq!(buf.point_count(), 0);
    }

    #[test]
    fn test_databuffer_get_channel() {
        let mut buf = DataBuffer::new(100, 3);
        buf.push_frame(&DataFrame::new(vec![10.0, 20.0, 30.0]));
        buf.push_frame(&DataFrame::new(vec![11.0, 21.0, 31.0]));
        assert_eq!(buf.get_channel(0, 2), vec![10.0, 11.0]);
        assert_eq!(buf.get_channel(2, 2), vec![30.0, 31.0]);
        // 越界返回空
        assert_eq!(buf.get_channel(99, 2), Vec::<f32>::new());
    }

    // ===== Derived buffer tests =====

    #[test]
    fn test_push_derived_aligned_with_timestamps() {
        // 场景: 3 帧, 每帧 push_frame 后 push_derived
        // 验证 derived 数据与 channels 时间戳对齐
        let mut buf = DataBuffer::new(100, 2);
        // 帧 0
        buf.push_frame(&DataFrame::new(vec![1.0, 2.0]));
        buf.push_derived("wave1", "math1", 10.0);
        // 帧 1
        buf.push_frame(&DataFrame::new(vec![3.0, 4.0]));
        buf.push_derived("wave1", "math1", 30.0);
        // 帧 2
        buf.push_frame(&DataFrame::new(vec![5.0, 6.0]));
        buf.push_derived("wave1", "math1", 50.0);

        let w = buf.get_recent(3);
        assert_eq!(w.channels[0], vec![1.0, 3.0, 5.0]);
        // derived 应与 channels 对齐
        let derived = w.derived.get("wave1").unwrap().get("math1").unwrap();
        assert_eq!(derived, &vec![10.0, 30.0, 50.0]);
    }

    #[test]
    fn test_derived_created_later_pads_nan() {
        // 场景: derived 缓冲区在第 2 帧才创建 (前 2 帧无 derived)
        let mut buf = DataBuffer::new(100, 1);
        buf.push_frame(&DataFrame::new(vec![1.0]));
        buf.push_frame(&DataFrame::new(vec![2.0]));
        // 第 3 帧才开始 push derived
        buf.push_frame(&DataFrame::new(vec![3.0]));
        buf.push_derived("wave1", "math1", 30.0);
        buf.push_frame(&DataFrame::new(vec![4.0]));
        buf.push_derived("wave1", "math1", 40.0);

        let w = buf.get_recent(4);
        assert_eq!(w.channels[0], vec![1.0, 2.0, 3.0, 4.0]);
        let derived = w.derived.get("wave1").unwrap().get("math1").unwrap();
        // 前 2 个应为 NaN, 后 2 个为实际值
        assert_eq!(derived.len(), 4);
        assert!(derived[0].is_nan());
        assert!(derived[1].is_nan());
        assert_eq!(derived[2], 30.0);
        assert_eq!(derived[3], 40.0);
    }

    #[test]
    fn test_multiple_derived_sources() {
        // 场景: 一个 sink 连接多个 source (math1, math2)
        let mut buf = DataBuffer::new(100, 1);
        buf.push_frame(&DataFrame::new(vec![1.0]));
        buf.push_derived("wave1", "math1", 10.0);
        buf.push_derived("wave1", "math2", 20.0);
        buf.push_frame(&DataFrame::new(vec![2.0]));
        buf.push_derived("wave1", "math1", 30.0);
        buf.push_derived("wave1", "math2", 40.0);

        let w = buf.get_recent(2);
        let sink_derived = w.derived.get("wave1").unwrap();
        assert_eq!(sink_derived.get("math1").unwrap(), &vec![10.0, 30.0]);
        assert_eq!(sink_derived.get("math2").unwrap(), &vec![20.0, 40.0]);
    }

    #[test]
    fn test_multiple_derived_sinks() {
        // 场景: 多个 sink 各自有 derived
        let mut buf = DataBuffer::new(100, 1);
        buf.push_frame(&DataFrame::new(vec![1.0]));
        buf.push_derived("wave1", "math1", 10.0);
        buf.push_derived("wave2", "math2", 20.0);

        let w = buf.get_recent(1);
        assert_eq!(
            w.derived.get("wave1").unwrap().get("math1").unwrap(),
            &vec![10.0]
        );
        assert_eq!(
            w.derived.get("wave2").unwrap().get("math2").unwrap(),
            &vec![20.0]
        );
    }

    #[test]
    fn test_clear_derived() {
        let mut buf = DataBuffer::new(100, 1);
        buf.push_frame(&DataFrame::new(vec![1.0]));
        buf.push_derived("wave1", "math1", 10.0);
        assert!(!buf.get_recent(1).derived.is_empty());

        buf.clear_derived();
        let w = buf.get_recent(1);
        assert!(w.derived.is_empty());
        // timestamps 和 channels 不受影响
        assert_eq!(w.channels[0], vec![1.0]);
    }

    #[test]
    fn test_remove_derived_sink() {
        let mut buf = DataBuffer::new(100, 1);
        buf.push_frame(&DataFrame::new(vec![1.0]));
        buf.push_derived("wave1", "math1", 10.0);
        buf.push_derived("wave2", "math2", 20.0);

        buf.remove_derived_sink("wave1");
        let w = buf.get_recent(1);
        assert!(!w.derived.contains_key("wave1"));
        assert!(w.derived.contains_key("wave2"));
    }

    #[test]
    fn test_derived_ringbuffer_overflow() {
        // 验证 derived 缓冲区也会覆盖旧数据
        let mut buf = DataBuffer::new(3, 1); // max_points = 3
        for i in 0..5 {
            buf.push_frame(&DataFrame::new(vec![i as f32]));
            buf.push_derived("wave1", "math1", (i * 10) as f32);
        }
        let w = buf.get_recent(3);
        // 只保留最近 3 个点
        assert_eq!(w.channels[0], vec![2.0, 3.0, 4.0]);
        let derived = w.derived.get("wave1").unwrap().get("math1").unwrap();
        assert_eq!(derived, &vec![20.0, 30.0, 40.0]);
    }

    #[test]
    fn test_get_window_with_derived() {
        // 验证 get_window 正切片 derived
        let mut buf = DataBuffer::new(100, 1);
        for i in 0..5 {
            buf.push_frame(&DataFrame::new(vec![i as f32]));
            buf.push_derived("wave1", "math1", (i * 10) as f32);
        }
        // 获取全部 (start_ms=0 表示从最新到最新, 但 end_ms 也为 0 会返回空)
        // 用 get_recent 更简单
        let w = buf.get_recent(5);
        assert_eq!(w.channels[0], vec![0.0, 1.0, 2.0, 3.0, 4.0]);
        let derived = w.derived.get("wave1").unwrap().get("math1").unwrap();
        assert_eq!(derived, &vec![0.0, 10.0, 20.0, 30.0, 40.0]);
    }

    #[test]
    fn test_derived_empty_buffer() {
        // 空 buffer 时 derived 也应为空
        let buf = DataBuffer::new(100, 2);
        let w = buf.get_recent(10);
        assert!(w.derived.is_empty());
    }

    // ===== RawDataCollector tests =====

    #[test]
    fn test_raw_collector_push_and_drain() {
        let mut col = RawDataCollector::with_capacity(1024);
        col.push_chunk(100, b"hello");
        col.push_chunk(200, b"world");
        assert_eq!(col.total_bytes(), 10);

        let batch = col.drain_batch(10);
        assert_eq!(batch.chunks.len(), 2);
        assert_eq!(batch.chunks[0].bytes, b"hello");
        assert_eq!(batch.chunks[1].bytes, b"world");
        assert_eq!(batch.total_bytes, 10);
        assert!(col.drain_batch(1024).chunks.is_empty());
    }

    #[test]
    fn test_raw_collector_drops_oldest() {
        let mut col = RawDataCollector::with_capacity(10);
        col.push_chunk(1, b"0123456789"); // 10 bytes, fits
        assert_eq!(col.dropped_bytes(), 0);
        col.push_chunk(2, b"xx"); // exceeds 10 bytes
        assert_eq!(col.dropped_bytes(), 10);

        let batch = col.drain_batch(1024);
        assert_eq!(batch.chunks.len(), 1);
        assert_eq!(batch.chunks[0].bytes, b"xx");
        assert_eq!(batch.dropped_bytes, 10);
    }

    #[test]
    fn test_raw_collector_drain_max_bytes() {
        let mut col = RawDataCollector::with_capacity(1024);
        col.push_chunk(1, b"12345");
        col.push_chunk(2, b"67890");
        col.push_chunk(3, b"abcde");

        // 只能完整取前两块 (10 bytes), 第三块 5 bytes 会超过 12
        let batch = col.drain_batch(12);
        assert_eq!(batch.chunks.len(), 2);
        assert_eq!(batch.total_bytes, 15);
    }

    #[test]
    fn test_raw_collector_clear() {
        let mut col = RawDataCollector::with_capacity(1024);
        col.push_chunk(1, b"data");
        col.clear();
        assert_eq!(col.total_bytes(), 0);
        assert_eq!(col.dropped_bytes(), 0);
        assert!(col.drain_batch(1024).chunks.is_empty());
    }
}
