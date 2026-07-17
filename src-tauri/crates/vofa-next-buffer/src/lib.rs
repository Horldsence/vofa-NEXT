//! # vofa-next-buffer
//!
//! 数据缓冲区与节点图路由。
//!
//! - [`RingBuffer`]: 泛型环形缓冲区
//! - [`DataBuffer`]: 多通道时间序列缓冲区
//! - [`NodeGraph`]: 节点连接关系管理 + 数据路由

pub mod graph;

pub use graph::{Edge, NodeGraph, RoutedData};

use serde::{Deserialize, Serialize};
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
}

/// 原始数据批次
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDataBatch {
    pub timestamp: u64,
    pub data: Vec<u8>,
    pub total_bytes: u64,
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
}

impl DataBuffer {
    pub fn new(max_points: usize, num_channels: usize) -> Self {
        let nc = num_channels.max(1);
        Self {
            channels: (0..nc).map(|_| RingBuffer::new(max_points)).collect(),
            timestamps: RingBuffer::new(max_points),
            max_points,
            num_channels: nc,
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

    /// 调整通道数 (仅增大, 保留已有数据)
    fn resize_channels(&mut self, new_count: usize) {
        while self.channels.len() < new_count {
            self.channels.push(RingBuffer::new(self.max_points));
        }
        self.num_channels = new_count;
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

        WaveformWindow {
            timestamps: window_ts,
            channels: window_channels,
            channel_count: self.num_channels,
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

        WaveformWindow {
            timestamps: rel_ts,
            channels,
            channel_count: self.num_channels,
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

    /// 清空
    pub fn clear(&mut self) {
        for ch in &mut self.channels {
            ch.clear();
        }
        self.timestamps.clear();
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
}
