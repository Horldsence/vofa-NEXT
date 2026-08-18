//! 原始字节收集器 (RawDataCollector) — 固定容量游标式历史缓冲
//!
//! 多数据源场景由 app 侧每个 Transport 节点 rx 各持一个实例
//! (FrameDecoder 节点旁路收集器独立管理, 见 app 侧 decoder_feed)。

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

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
pub(crate) struct StoredChunk {
    pub(crate) timestamp_us: u64,
    pub(crate) direction: RawDataDirection,
    pub(crate) bytes: Vec<u8>,
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
        self.chunks.clear();
        self.stored = 0;
        self.total_bytes = 0;
        self.dropped_bytes = 0;
        self.base_index = 0;
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
    pub fn stored_bytes(&self) -> usize {
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
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// 累计丢弃字节数
    pub fn dropped_bytes(&self) -> u64 {
        self.dropped_bytes
    }
}

impl Default for RawDataCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_collector_push_and_read() {
        let mut col = RawDataCollector::with_capacity(1024);
        col.push_chunk(100, RawDataDirection::Rx, b"hello");
        col.push_chunk(200, RawDataDirection::Rx, b"world");
        assert_eq!(col.total_bytes(), 10);
        assert_eq!(col.chunk_count(), 2);

        let (chunks, next) = col.read_from(0, 10);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].2, b"hello");
        assert_eq!(chunks[1].2, b"world");
        assert_eq!(next, 2);

        // 再次读取同一范围仍可见 (不消费)
        let (chunks2, next2) = col.read_from(0, 10);
        assert_eq!(chunks2.len(), 2);
        assert_eq!(next2, 2);
    }

    #[test]
    fn test_raw_collector_drops_oldest() {
        let mut col = RawDataCollector::with_capacity(10);
        col.push_chunk(1, RawDataDirection::Rx, b"0123456789"); // 10 bytes, fits
        assert_eq!(col.dropped_bytes(), 0);
        col.push_chunk(2, RawDataDirection::Rx, b"xx"); // exceeds 10 bytes
        assert_eq!(col.dropped_bytes(), 10);
        assert_eq!(col.base_index(), 1);

        let (chunks, next) = col.read_from(0, 1024);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].2, b"xx");
        assert_eq!(next, 2);
    }

    #[test]
    fn test_raw_collector_read_max_bytes() {
        let mut col = RawDataCollector::with_capacity(1024);
        col.push_chunk(1, RawDataDirection::Rx, b"12345");
        col.push_chunk(2, RawDataDirection::Rx, b"67890");
        col.push_chunk(3, RawDataDirection::Rx, b"abcde");

        // 只能完整取前两块 (10 bytes), 第三块 5 bytes 会超过 12
        let (chunks, next) = col.read_from(0, 12);
        assert_eq!(chunks.len(), 2);
        assert_eq!(next, 2);
    }

    #[test]
    fn test_raw_collector_clear() {
        let mut col = RawDataCollector::with_capacity(1024);
        col.push_chunk(1, RawDataDirection::Rx, b"data");
        col.clear();
        assert_eq!(col.total_bytes(), 0);
        assert_eq!(col.dropped_bytes(), 0);
        assert_eq!(col.base_index(), 0);
        let (chunks, _) = col.read_from(0, 1024);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_raw_collector_read_filtered() {
        let mut col = RawDataCollector::with_capacity(1024);
        col.push_chunk(1, RawDataDirection::Rx, b"hello");
        col.push_chunk(2, RawDataDirection::Tx, b"world");
        col.push_chunk(3, RawDataDirection::Rx, b"again");

        // 仅 RX
        let (chunks, next) = col.read_filtered_from(0, 1024, DirectionFilter::Rx, None);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].2, b"hello");
        assert_eq!(chunks[1].2, b"again");
        assert_eq!(next, 3);

        // 仅 TX
        let (chunks, _) = col.read_filtered_from(0, 1024, DirectionFilter::Tx, None);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].2, b"world");

        // 搜索 "ell"
        let pat = SearchPattern::parse("ell").unwrap();
        let (chunks, _) = col.read_filtered_from(0, 1024, DirectionFilter::All, Some(&pat));
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].2, b"hello");

        // hex 搜索 "77 6f" (wo)
        let pat = SearchPattern::parse("77 6f").unwrap();
        let (chunks, _) = col.read_filtered_from(0, 1024, DirectionFilter::All, Some(&pat));
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].2, b"world");
    }
}
