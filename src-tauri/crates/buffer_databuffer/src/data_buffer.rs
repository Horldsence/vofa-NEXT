//! `DataBuffer` 核心 — 多通道 f32 时间序列 + 版本号

use std::collections::HashMap;

use buffer_ring::RingBuffer;
use vofa_core::DataFrame;

use crate::derived::DerivedEntry;

/// 多通道时间序列数据缓冲区
///
/// 多数据源场景由 app 侧每源一个实例实现 (本类型语义不变);
/// 派生键 (sink, source) 随实例天然隔离。
pub struct DataBuffer {
    /// 每通道一个环形缓冲区
    pub(crate) channels: Vec<RingBuffer<f32>>,
    /// 时间戳缓冲区 (微秒)
    pub(crate) timestamps: RingBuffer<u64>,
    /// 最大点数
    pub(crate) max_points: usize,
    /// 当前通道数 (可动态变化)
    pub(crate) num_channels: usize,
    /// 派生数据缓冲 (稳定索引直写): 批首注册拿索引, 逐帧零哈希写入。
    /// 与 timestamps 同步 push, 保证时间戳完全对齐
    pub(crate) derived_list: Vec<DerivedEntry>,
    /// (sink, source) → derived_list 下标
    pub(crate) derived_index: HashMap<(String, String), usize>,
    /// 单调递增版本号 — push_frame/push_derived 时变化, 供订阅循环做变化检测
    pub(crate) version: u64,
}

impl DataBuffer {
    pub fn new(max_points: usize, num_channels: usize) -> Self {
        let nc = num_channels.max(1);
        Self {
            channels: (0..nc).map(|_| RingBuffer::new(max_points)).collect(),
            timestamps: RingBuffer::new(max_points),
            max_points,
            num_channels: nc,
            derived_list: Vec::new(),
            derived_index: HashMap::new(),
            version: 0,
        }
    }

    /// 当前版本号 (单调递增, 数据变化时递增)
    pub const fn version(&self) -> u64 {
        self.version
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
            // 通道缺失使用 NaN 保持时间轴对齐，但绝不能伪装成真实零值。
            let val = frame.channels.get(i).copied().unwrap_or(f32::NAN);
            self.channels[i].push(val);
        }
        self.version = self.version.wrapping_add(1);
    }

    /// 调整通道数 (仅增大, 保留已有数据)
    fn resize_channels(&mut self, new_count: usize) {
        while self.channels.len() < new_count {
            self.channels.push(RingBuffer::new(self.max_points));
        }
        self.num_channels = new_count;
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
        for e in &mut self.derived_list {
            e.rb.resize(new_max);
        }
    }

    /// 清空
    pub fn clear(&mut self) {
        for ch in &mut self.channels {
            ch.clear();
        }
        self.timestamps.clear();
        self.derived_list.clear();
        self.derived_index.clear();
    }

    /// 设置通道数 (清空已有数据)
    pub fn set_channels(&mut self, count: usize) {
        let nc = count.max(1);
        self.channels = (0..nc).map(|_| RingBuffer::new(self.max_points)).collect();
        self.timestamps.clear();
        self.num_channels = nc;
    }
}
