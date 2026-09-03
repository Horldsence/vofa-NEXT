//! 原始字节收集器 (RawDataCollector) — 固定容量游标式历史缓冲
//!
//! 多数据源场景由 app 侧每个 Transport 节点 rx 各持一个实例
//! (FrameDecoder 节点旁路收集器独立管理, 见 app 侧 decoder_feed)。

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::raw_filter::{chunk_matches, DirectionFilter, SearchPattern};

/// 原始数据方向 — 接收 (RX) 或发送 (TX)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum RawDataDirection {
    /// 接收数据
    #[default]
    #[serde(rename = "rx")]
    Rx,
    /// 发送数据
    #[serde(rename = "tx")]
    Tx,
}

/// 原始数据块 (线上格式) — 字节走 base64 而非 JSON 数字数组
///
/// JSON 数字数组膨胀约 3.5x (每个字节 1~4 字符 + 逗号), base64 仅 1.37x,
/// 且解码在 JS 侧可用 atob 一次完成, 是 RAWDATA 高通量 (7MB/s+) 的关键。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDataChunk {
    pub timestamp_us: u64,
    /// 数据方向 — rx 接收 / tx 发送
    #[serde(default)]
    pub direction: RawDataDirection,
    /// base64 编码的原始字节
    pub bytes_b64: String,
}

/// 原始数据批次
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDataBatch {
    /// 全局单调序号 — 分片并发推送时由分发器在 drain 时分配,
    /// 前端按 seq 重组, 保证字节序与到达顺序一致 (单分片时从 0 连续递增)
    #[serde(default)]
    pub seq: u64,
    pub chunks: Vec<RawDataChunk>,
    pub total_bytes: u64,
    pub dropped_bytes: u64,
}

/// 内部存储块 (未编码的原始字节)
#[derive(Debug, Clone)]
pub struct StoredChunk {
    pub timestamp_us: u64,
    pub direction: RawDataDirection,
    pub bytes: Vec<u8>,
}

/// [`RawDataCollector::drain_raw`] 的返回 — 未编码的原始块 + 计数器快照
///
/// 编码延迟到 [`RawDrain::into_batch`] 在 collector 锁外进行。
#[derive(Debug)]
pub struct RawDrain {
    /// (timestamp_us, direction, bytes)
    pub chunks: Vec<(u64, RawDataDirection, Vec<u8>)>,
    pub total_bytes: u64,
    pub dropped_bytes: u64,
}

impl RawDrain {
    /// 编码为线上批次 (base64) — 在 collector 锁外调用
    pub fn into_batch(self) -> RawDataBatch {
        use base64::Engine;
        RawDataBatch {
            seq: 0, // 由分发器在发送前统一分配
            chunks: self
                .chunks
                .into_iter()
                .map(|(timestamp_us, direction, bytes)| RawDataChunk {
                    timestamp_us,
                    direction,
                    bytes_b64: base64::engine::general_purpose::STANDARD.encode(bytes),
                })
                .collect(),
            total_bytes: self.total_bytes,
            dropped_bytes: self.dropped_bytes,
        }
    }
}

/// 原始数据收集器 — 固定容量游标式历史缓冲区, 读取不消费
///
/// 所有订阅者通过 `read_from` / `read_filtered_from` 按绝对索引游标读取,
/// 容量溢出时丢弃最旧块并推进 `base_index`, 游标失效时自动对齐到 `base_index`。
#[derive(Debug, Clone)]
pub struct RawDataCollector {
    chunks: VecDeque<StoredChunk>,
    capacity: usize,
    /// 当前缓存字节数 (增量维护, 避免每次 O(n) 遍历)
    stored: usize,
    total_bytes: u64,
    dropped_bytes: u64,
    /// chunks[0] 对应的绝对索引 — 容量溢出丢弃最旧块时递增
    base_index: usize,
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
            stored: 0,
            total_bytes: 0,
            dropped_bytes: 0,
            base_index: 0,
        }
    }

    /// 推入一块原始数据; 若超出容量则丢弃最旧块并推进 base_index
    pub fn push_chunk(&mut self, timestamp_us: u64, direction: RawDataDirection, bytes: &[u8]) {
        self.total_bytes += bytes.len() as u64;
        self.stored += bytes.len();
        self.chunks.push_back(StoredChunk {
            timestamp_us,
            direction,
            bytes: bytes.to_vec(),
        });

        while self.stored > self.capacity && !self.chunks.is_empty() {
            if let Some(front) = self.chunks.pop_front() {
                self.stored -= front.bytes.len();
                self.dropped_bytes += front.bytes.len() as u64;
                self.base_index += 1;
            }
        }
    }

    /// 从指定绝对索引开始只读读取, 不消费数据
    ///
    /// 返回 (chunks, next_index):
    /// - chunks: 从 start_index 开始的若干完整块, 累计字节数不超过 max_bytes
    /// - next_index: 下一次读取的起始索引
    ///
    /// start_index 小于 base_index 时自动对齐到 base_index (数据已被丢弃)。
    pub fn read_from(
        &self,
        start_index: usize,
        max_bytes: usize,
    ) -> (Vec<(u64, RawDataDirection, Vec<u8>)>, usize) {
        let start = start_index.max(self.base_index);
        let rel_start = start.saturating_sub(self.base_index);
        let mut result = Vec::new();
        let mut acc = 0usize;
        let mut next = start;

        for chunk in self.chunks.iter().skip(rel_start) {
            let next_acc = acc.saturating_add(chunk.bytes.len());
            if next_acc > max_bytes && !result.is_empty() {
                break;
            }
            acc = next_acc;
            result.push((chunk.timestamp_us, chunk.direction, chunk.bytes.clone()));
            next += 1;
        }

        (result, next)
    }

    /// 从指定绝对索引开始只读读取, 并按方向与搜索模式过滤
    ///
    /// 与 read_from 类似, 但只返回方向匹配且包含搜索模式的 chunk。
    /// 搜索模式支持跨 chunk 边界匹配 (通过保留上一个匹配 chunk 的尾部字节)。
    pub fn read_filtered_from(
        &self,
        start_index: usize,
        max_bytes: usize,
        direction: DirectionFilter,
        pattern: Option<&SearchPattern>,
    ) -> (Vec<(u64, RawDataDirection, Vec<u8>)>, usize) {
        let start = start_index.max(self.base_index);
        let rel_start = start.saturating_sub(self.base_index);
        let mut result = Vec::new();
        let mut acc = 0usize;
        let mut next = start;
        let mut prev_tail: Vec<u8> = Vec::new();

        for chunk in self.chunks.iter().skip(rel_start) {
            let next_acc = acc.saturating_add(chunk.bytes.len());
            if next_acc > max_bytes && !result.is_empty() {
                break;
            }
            acc = next_acc;

            let (matches, new_tail) = chunk_matches(chunk, direction, pattern, &prev_tail);
            if matches {
                result.push((chunk.timestamp_us, chunk.direction, chunk.bytes.clone()));
            }
            // 只有方向匹配的数据才有资格作为跨 chunk tail,
            // 因为过滤后的流中不存在方向不匹配的字节。
            if direction.matches(chunk.direction) {
                prev_tail = new_tail;
            }
            next += 1;
        }

        (result, next)
    }

    /// 当前最早可读绝对索引
    pub const fn base_index(&self) -> usize {
        self.base_index
    }

    /// 当前缓存的块数量
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// 清空所有块并重置计数器
    pub fn clear(&mut self) {
        // base_index 是活跃订阅游标所在的绝对空间。清空时推进到下一块，不能归零；
        // 否则已有 RawDataSource 的 read_index 会落在新流未来的位置，重连后长期读不到数据。
        self.base_index = self.base_index.saturating_add(self.chunks.len());
        self.chunks.clear();
        self.stored = 0;
        self.total_bytes = 0;
        self.dropped_bytes = 0;
    }

    /// 设置容量 (保留最近块)
    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
        while self.stored > self.capacity && !self.chunks.is_empty() {
            if let Some(front) = self.chunks.pop_front() {
                self.stored -= front.bytes.len();
                self.dropped_bytes += front.bytes.len() as u64;
                self.base_index += 1;
            }
        }
    }

    /// 当前缓存字节数 (供自适应批量/分片扩缩容做积压检测)
    pub const fn stored_bytes(&self) -> usize {
        self.stored
    }

    /// 从指定绝对索引到最新的未读字节数 (游标式订阅的真实积压)
    ///
    /// 与 stored_bytes 的区别: stored_bytes 是全量存储 (环形满时恒定),
    /// 本方法才是该游标还未消费的量, 分片扩缩容应以此为准。
    pub fn remaining_bytes_from(&self, index: usize) -> usize {
        let start = index.max(self.base_index);
        let rel = start.saturating_sub(self.base_index);
        self.chunks.iter().skip(rel).map(|c| c.bytes.len()).sum()
    }

    /// 累计写入字节数 (含已丢弃)
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// 累计丢弃字节数
    pub const fn dropped_bytes(&self) -> u64 {
        self.dropped_bytes
    }
}

impl Default for RawDataCollector {
    fn default() -> Self {
        Self::new()
    }
}
