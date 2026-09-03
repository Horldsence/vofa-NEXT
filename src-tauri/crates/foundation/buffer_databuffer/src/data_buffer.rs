//! `DataBuffer` 核心 — 多通道 f32 时间序列 + 版本号
//!
//! 记录平面专属 (数据平面不变量 3): 原始通道由字节平面/记录路径写入,
//! 求值平面的派生通道在独立时间轴上 ([`DerivedStore`], 内部独立 Mutex) —
//! 两个平面各持一把锁互不阻塞, 求值积压不影响原始波形入库。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use buffer_ring::RingBuffer;
use parking_lot::Mutex;
use vofa_core::DataFrame;

use crate::derived::{shared_derived_store, SharedDerivedStore};
use crate::tier::Tier;

/// 多通道时间序列数据缓冲区
///
/// 多数据源场景由 app 侧每源一个实例实现 (本类型语义不变);
/// 派生键 (sink, source, source_handle) 随实例天然隔离。
///
/// `Clone` 为深拷贝 (派生存储一并冻结) — 停止态快照依赖该语义。
pub struct DataBuffer {
    /// 每通道一个环形缓冲区
    pub(crate) channels: Vec<RingBuffer<f32>>,
    /// 时间戳缓冲区 (微秒)
    pub(crate) timestamps: RingBuffer<u64>,
    /// 最大点数
    pub(crate) max_points: usize,
    /// 当前通道数 (可动态变化)
    pub(crate) num_channels: usize,
    /// 原始通道版本号 — push_frame 时递增
    pub(crate) version: u64,
    /// 派生通道存储 (求值平面写入, 独立时间轴 + 独立锁)
    pub(crate) derived: SharedDerivedStore,
    /// 派生版本号快照 (避免每次 version() 加锁 — 与派生锁内版本单调合并)
    pub(crate) derived_version: AtomicU64,
    /// min-max 金字塔层 (L1 起; L0 即 channels + timestamps 本体)
    pub(crate) tiers: Vec<Tier>,
    /// L0 逻辑推送总数 (折叠节拍用, 环形覆盖不改写)
    pub(crate) raw_pushed: u64,
    /// L0 滚动覆盖丢弃的样本数 (显式降载计数, 不变量 5)
    pub(crate) storage_overflow: u64,
}

impl Clone for DataBuffer {
    fn clone(&self) -> Self {
        // 深拷贝派生存储与金字塔: 克隆即冻结 (停止态快照语义)
        let snapshot = self.derived.lock().clone();
        Self {
            channels: self.channels.clone(),
            timestamps: self.timestamps.clone(),
            max_points: self.max_points,
            num_channels: self.num_channels,
            version: self.version,
            derived: Arc::new(Mutex::new(snapshot)),
            derived_version: AtomicU64::new(self.derived_version.load(Ordering::Relaxed)),
            tiers: self.tiers.clone(),
            raw_pushed: self.raw_pushed,
            storage_overflow: self.storage_overflow,
        }
    }
}

impl DataBuffer {
    pub fn new(max_points: usize, num_channels: usize) -> Self {
        let nc = num_channels.max(1);
        Self {
            channels: (0..nc).map(|_| RingBuffer::new(max_points)).collect(),
            timestamps: RingBuffer::new(max_points),
            max_points,
            num_channels: nc,
            version: 0,
            derived: shared_derived_store(),
            derived_version: AtomicU64::new(0),
            tiers: Vec::new(),
            raw_pushed: 0,
            storage_overflow: 0,
        }
    }

    /// 当前版本号 (单调递增; 原始与派生写入均会推进, 供订阅循环变化检测)
    pub fn version(&self) -> u64 {
        self.version
            .wrapping_add(self.derived_version.load(Ordering::Relaxed))
    }

    /// 推入一帧数据
    pub fn push_frame(&mut self, frame: &DataFrame) {
        self.push_frame_at(frame.timestamp, &frame.channels);
    }

    /// 以显式时间戳推入一帧数据
    ///
    /// 时间戳由字节平面采样时钟权威给定 (单一时钟域), 本方法原样入库。
    /// L0 满则滚动覆盖最旧 (`storage_overflow` 计数), 金字塔层保留其包络摘要
    /// — 不变量 2: 超限走分层降载, 不走静默丢弃。
    pub fn push_frame_at(&mut self, timestamp: u64, channels: &[f32]) {
        // 动态调整通道数
        let frame_ch = channels.len();
        if frame_ch > self.num_channels {
            self.resize_channels(frame_ch);
        }
        if self.timestamps.len() >= self.max_points {
            self.storage_overflow = self.storage_overflow.wrapping_add(1);
        }
        self.timestamps.push(timestamp);
        for i in 0..self.num_channels {
            // 通道缺失使用 NaN 保持时间轴对齐，但绝不能伪装成真实零值。
            let val = channels.get(i).copied().unwrap_or(f32::NAN);
            self.channels[i].push(val);
        }
        self.version = self.version.wrapping_add(1);
        self.maybe_fold();
    }

    /// L0 滚动覆盖丢弃的样本累计数 (WWB1 元数据/诊断用)
    pub const fn storage_overflow(&self) -> u64 {
        self.storage_overflow
    }

    /// 容量自洽 (不变量 2): 按来源名义速率 × 目标窗口整备 L0 容量, 受 `cap` 封顶。
    /// 幂等 — 目标容量不大于当前容量时不动作。返回是否发生扩容。
    pub fn ensure_capacity_for_rate(&mut self, rate_hz: f64, window_s: f64, cap: usize) -> bool {
        let target = (rate_hz * window_s).ceil();
        if !target.is_finite() || target <= 0.0 {
            return false;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // target 已 ceil 且判定 is_finite/正, 截断无害
        let target = (target as usize).clamp(100_000, cap.max(100_000));
        if target > self.max_points {
            self.set_max_points(target);
            true
        } else {
            false
        }
    }

    /// 调整通道数 (仅增大, 保留已有数据; 金字塔层按新通道布局重建)
    fn resize_channels(&mut self, new_count: usize) {
        while self.channels.len() < new_count {
            self.channels.push(RingBuffer::new(self.max_points));
        }
        self.num_channels = new_count;
        self.tiers.clear();
        self.raw_pushed = 0;
    }

    /// 获取单通道最近 N 个点
    pub fn get_channel(&self, ch: usize, count: usize) -> Vec<f32> {
        if ch >= self.channels.len() {
            return Vec::new();
        }
        self.channels[ch].recent(count)
    }

    /// 当前通道数
    pub const fn channel_count(&self) -> usize {
        self.num_channels
    }

    /// 当前点数
    pub const fn point_count(&self) -> usize {
        self.timestamps.len()
    }

    /// 最大容量 (点)
    pub const fn max_points(&self) -> usize {
        self.max_points
    }

    /// 当前缓冲区实际分配的主要时序数据字节数估算，供冻结快照预算使用。
    pub fn estimated_bytes(&self) -> usize {
        let series = 1usize.saturating_add(self.num_channels);
        let raw = self
            .max_points
            .saturating_mul(8usize.saturating_add(series.saturating_sub(1).saturating_mul(4)));
        let derived = self.derived.lock().estimated_bytes(self.max_points);
        raw.saturating_add(derived)
    }

    /// 设置最大容量 (保留最近数据; 金字塔层按新容量重建)
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
        self.derived.lock().resize(new_max);
        self.tiers.clear();
        self.raw_pushed = 0;
    }

    /// 清空 (version 不变 — 清除不作为数据变化推送给显示订阅)
    pub fn clear(&mut self) {
        for ch in &mut self.channels {
            ch.clear();
        }
        self.timestamps.clear();
        self.derived.lock().clear();
        self.tiers.clear();
        self.raw_pushed = 0;
    }

    /// 设置通道数 (清空已有数据)
    pub fn set_channels(&mut self, count: usize) {
        let nc = count.max(1);
        self.channels = (0..nc).map(|_| RingBuffer::new(self.max_points)).collect();
        self.timestamps.clear();
        self.num_channels = nc;
        self.tiers.clear();
        self.raw_pushed = 0;
    }
}
