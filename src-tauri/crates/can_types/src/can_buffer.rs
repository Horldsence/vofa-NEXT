//! CAN 帧环形缓冲区 — 覆盖最旧数据 + 版本号 + 增量游标读取

use std::collections::VecDeque;

use crate::can_frame::CanFrame;

/// CAN 帧环形缓冲区
///
/// - 容量限制:超出 `max_size` 时丢弃最旧帧
/// - 版本号:每次 `push` 单调递增,用于跨 shard 同步
/// - 增量游标:`drain_from` 支持订阅流按 `version` 增量拉取
#[derive(Debug)]
pub struct CanBuffer {
    frames: VecDeque<CanFrame>,
    max_size: usize,
    version: u64,
}

impl CanBuffer {
    /// 创建指定容量的 CAN 缓冲区(底层预分配至少 8192 帧)。
    ///
    /// `max_size` 强制下限为 1,避免 0 容量的退化状态。
    pub fn new(max_size: usize) -> Self {
        let max_size = max_size.max(1);
        Self {
            frames: VecDeque::with_capacity(max_size.min(8192)),
            max_size,
            version: 0,
        }
    }

    /// 推入一帧(超出容量时丢弃最旧帧)
    pub fn push(&mut self, frame: CanFrame) {
        if self.frames.len() >= self.max_size {
            self.frames.pop_front();
        }
        self.frames.push_back(frame);
        self.version = self.version.wrapping_add(1);
    }

    /// 当前版本号(单调递增,push 时变化)
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 获取最近 n 帧(按时间顺序返回,旧的在前)
    pub fn get_recent(&self, count: usize) -> Vec<CanFrame> {
        let n = count.min(self.frames.len());
        self.frames.iter().rev().take(n).rev().cloned().collect()
    }

    /// 增量游标读取 — 统一分片流用
    ///
    /// cursor 为绝对序号(`version` = 累计 push 数)。可读区间 = `[max(cursor, version-len), version)`。
    /// 游标若已被驱逐越过则顺移并计入 `dropped`。
    /// 返回 `(items, new_cursor, dropped)`。
    ///
    /// 行为规约:
    /// - `cursor >= version`: 返回当前全部帧;`new_cursor = cursor`(不回退);
    ///   `dropped = cursor - version`(代表 cursor 与当前之间已跳过的逻辑帧数)。
    /// - `cursor < version`: 从 `max(cursor, oldest)` 读取最多 `max` 帧;
    ///   `dropped = start - cursor`(cursor 已被驱逐的部分)。
    pub fn drain_from(&self, cursor: u64, max: usize) -> (Vec<CanFrame>, u64, u64) {
        let version = self.version;
        // 已完全领先:给出当前缓冲,光标不回退
        if cursor >= version {
            let items: Vec<CanFrame> = self.frames.iter().cloned().collect();
            return (items, cursor, cursor - version);
        }
        let len = self.frames.len() as u64;
        let oldest = version.saturating_sub(len);
        let start = cursor.max(oldest);
        let dropped = start.saturating_sub(cursor);
        let n = usize::try_from(version - start).unwrap_or(0).min(max);
        let skip = usize::try_from(start.saturating_sub(oldest)).unwrap_or(0);
        let items = self.frames.iter().skip(skip).take(n).cloned().collect();
        (items, start + n as u64, dropped)
    }

    /// 清空缓冲区(版本号不清零,继续递增以与外部订阅同步)
    pub fn clear(&mut self) {
        self.frames.clear();
        self.version = self.version.wrapping_add(1);
    }

    /// 设置最大容量(保留最近帧)
    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size.max(1);
        while self.frames.len() > self.max_size {
            self.frames.pop_front();
        }
    }

    /// 当前缓冲区中的帧数
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// 缓冲区是否为空
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// 最大容量
    pub const fn capacity(&self) -> usize {
        self.max_size
    }
}
