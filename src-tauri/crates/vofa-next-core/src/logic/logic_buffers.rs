//! 逻辑分析仪环形缓冲区

use std::collections::VecDeque;

use super::{DecodedEvent, LogicSample};

/// 逻辑采样环形缓冲区 — 用于前端订阅查询
pub struct LogicBuffer {
    samples: VecDeque<LogicSample>,
    max_size: usize,
    /// 单调递增版本号 — 每次 push +1, 供订阅循环做变化检测
    version: u64,
}

impl LogicBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_size.min(16384)),
            max_size,
            version: 0,
        }
    }

    pub fn push(&mut self, sample: LogicSample) {
        if self.samples.len() >= self.max_size {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
        self.version = self.version.wrapping_add(1);
    }

    /// 当前版本号 (单调递增, push 时变化)
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 获取最近 n 个采样 (返回顺序: 旧→新)
    pub fn get_recent(&self, count: usize) -> Vec<LogicSample> {
        let n = count.min(self.samples.len());
        self.samples
            .iter()
            .rev()
            .take(n)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// 增量游标读取 — 统一分片流用 (语义同 CanBuffer::drain_from)
    pub fn drain_from(&self, cursor: u64, max: usize) -> (Vec<LogicSample>, u64, u64) {
        let version = self.version;
        let oldest = version - self.samples.len() as u64;
        let start = cursor.max(oldest);
        let dropped = start.saturating_sub(cursor);
        let n = usize::try_from(version - start).unwrap_or(max).min(max);
        let skip = usize::try_from(start - oldest).unwrap_or(0);
        let items = self.samples.iter().skip(skip).take(n).cloned().collect();
        (items, start + n as u64, dropped)
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size;
        while self.samples.len() > max_size {
            self.samples.pop_front();
        }
    }
}

/// 解码事件环形缓冲区
pub struct DecodedBuffer {
    events: VecDeque<DecodedEvent>,
    max_size: usize,
    /// 单调递增版本号 — 每次 push +1, 供订阅循环做变化检测
    version: u64,
}

impl DecodedBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(max_size.min(8192)),
            max_size,
            version: 0,
        }
    }

    pub fn push(&mut self, event: DecodedEvent) {
        if self.events.len() >= self.max_size {
            self.events.pop_front();
        }
        self.events.push_back(event);
        self.version = self.version.wrapping_add(1);
    }

    /// 当前版本号 (单调递增, push 时变化)
    pub const fn version(&self) -> u64 {
        self.version
    }

    pub fn get_recent(&self, count: usize) -> Vec<DecodedEvent> {
        let n = count.min(self.events.len());
        self.events
            .iter()
            .rev()
            .take(n)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// 增量游标读取 — 统一分片流用 (语义同 CanBuffer::drain_from)
    pub fn drain_from(&self, cursor: u64, max: usize) -> (Vec<DecodedEvent>, u64, u64) {
        let version = self.version;
        let oldest = version - self.events.len() as u64;
        let start = cursor.max(oldest);
        let dropped = start.saturating_sub(cursor);
        let n = usize::try_from(version - start).unwrap_or(max).min(max);
        let skip = usize::try_from(start - oldest).unwrap_or(0);
        let items = self.events.iter().skip(skip).take(n).cloned().collect();
        (items, start + n as u64, dropped)
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size.max(1);
        while self.events.len() > self.max_size {
            self.events.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}
