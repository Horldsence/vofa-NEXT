//! 逻辑分析仪环形缓冲区 — `LogicBuffer`(采样) + `DecodedBuffer`(解码事件)
//!
//! 两个 buffer 共享一致 API(版本号、`push`、`get_recent`、`drain_from`、
//! `set_max_size`、`clear`、`len`/`is_empty`),便于上层订阅循环统一处理。
//!
//! `drain_from` 语义与 [`crate::can_types::CanBuffer::drain_from`] 对齐。

use std::collections::VecDeque;

use crate::types::{DecodedEvent, LogicSample};

// ============ 公共工具 ============

/// 通用增量游标读取逻辑(由 LogicBuffer / DecodedBuffer 复用)
fn drain_cursor<T: Clone>(
    items: &VecDeque<T>,
    version: u64,
    cursor: u64,
    max: usize,
) -> (Vec<T>, u64, u64) {
    if cursor >= version {
        let items: Vec<T> = items.iter().cloned().collect();
        return (items, cursor, cursor - version);
    }
    let len = items.len() as u64;
    let oldest = version.saturating_sub(len);
    let start = cursor.max(oldest);
    let dropped = start.saturating_sub(cursor);
    let n = usize::try_from(version - start).unwrap_or(0).min(max);
    let skip = usize::try_from(start.saturating_sub(oldest)).unwrap_or(0);
    let out: Vec<T> = items.iter().skip(skip).take(n).cloned().collect();
    (out, start + n as u64, dropped)
}

// ============ LogicBuffer(采样) ============

/// 逻辑采样环形缓冲区 — 用于前端订阅查询
#[derive(Debug)]
pub struct LogicBuffer {
    samples: VecDeque<LogicSample>,
    max_size: usize,
    /// 单调递增版本号 — 每次 push +1, 供订阅循环做变化检测
    version: u64,
}

impl LogicBuffer {
    /// 创建指定容量的采样缓冲区
    pub fn new(max_size: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_size.min(16384)),
            max_size,
            version: 0,
        }
    }

    /// 推入一个采样,容量满时丢弃最旧数据
    pub fn push(&mut self, sample: LogicSample) {
        if self.samples.len() >= self.max_size {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
        self.version = self.version.wrapping_add(1);
    }

    /// 当前版本号(单调递增,push 时变化)
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 获取最近 n 个采样(返回顺序: 旧→新)
    pub fn get_recent(&self, count: usize) -> Vec<LogicSample> {
        let n = count.min(self.samples.len());
        let mut out: Vec<LogicSample> = self.samples.iter().rev().take(n).cloned().collect();
        out.reverse();
        out
    }

    /// 增量游标读取 — 统一分片流用(语义同 `CanBuffer::drain_from`)
    pub fn drain_from(&self, cursor: u64, max: usize) -> (Vec<LogicSample>, u64, u64) {
        drain_cursor(&self.samples, self.version, cursor, max)
    }

    /// 清空缓冲区
    pub fn clear(&mut self) {
        self.samples.clear();
        self.version = self.version.wrapping_add(1);
    }

    /// 当前采样数
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// 是否空
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// 最大容量
    pub const fn max_size(&self) -> usize {
        self.max_size
    }

    /// 设置最大容量(保留最近帧)
    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size.max(1);
        while self.samples.len() > self.max_size {
            self.samples.pop_front();
        }
    }
}

// ============ DecodedBuffer(解码事件) ============

/// 解码事件环形缓冲区
#[derive(Debug)]
pub struct DecodedBuffer {
    events: VecDeque<DecodedEvent>,
    max_size: usize,
    /// 单调递增版本号 — 每次 push +1, 供订阅循环做变化检测
    version: u64,
}

impl DecodedBuffer {
    /// 创建指定容量的事件缓冲区
    pub fn new(max_size: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(max_size.min(8192)),
            max_size,
            version: 0,
        }
    }

    /// 推入一个解码事件,容量满时丢弃最旧数据
    pub fn push(&mut self, event: DecodedEvent) {
        if self.events.len() >= self.max_size {
            self.events.pop_front();
        }
        self.events.push_back(event);
        self.version = self.version.wrapping_add(1);
    }

    /// 当前版本号(单调递增,push 时变化)
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 获取最近 n 个事件(返回顺序: 旧→新)
    pub fn get_recent(&self, count: usize) -> Vec<DecodedEvent> {
        let n = count.min(self.events.len());
        let mut out: Vec<DecodedEvent> = self.events.iter().rev().take(n).cloned().collect();
        out.reverse();
        out
    }

    /// 增量游标读取 — 统一分片流用(语义同 `CanBuffer::drain_from`)
    pub fn drain_from(&self, cursor: u64, max: usize) -> (Vec<DecodedEvent>, u64, u64) {
        drain_cursor(&self.events, self.version, cursor, max)
    }

    /// 清空缓冲区
    pub fn clear(&mut self) {
        self.events.clear();
        self.version = self.version.wrapping_add(1);
    }

    /// 当前事件数
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// 是否空
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// 最大容量
    pub const fn max_size(&self) -> usize {
        self.max_size
    }

    /// 设置最大容量
    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size.max(1);
        while self.events.len() > self.max_size {
            self.events.pop_front();
        }
    }
}
