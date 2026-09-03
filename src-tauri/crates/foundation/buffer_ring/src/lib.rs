//! # buffer_ring
//!
//! 泛型环形缓冲区 — 固定容量, 覆盖最旧数据。
//!
//! 提供 Layer 0 基础设施, 被 `buffer_databuffer` / `buffer_raw` 等上层 crate
//! 用作时间序列/原始块的底层容器。本 crate **零外部依赖**。

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
    pub const fn len(&self) -> usize {
        self.len
    }

    /// 是否为空
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 容量
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// 清空
    pub const fn clear(&mut self) {
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
