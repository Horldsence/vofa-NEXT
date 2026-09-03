//! # filtered_sources — 带过滤条件的数据流 Source
//!
//! 与 `stream.rs` 中的基础 Source 对应, 但增加过滤条件:
//! drain 后按过滤器筛选, 只把匹配的条目推给前端。
//!
//! 所有 Source 都接入现有 `sharded_stream_loop`, 自动获得分片并发能力。

use buffer_raw::{DirectionFilter, RawDataBatch, RawDataCollector, RawDrain, SearchPattern};
use can_types::{CanBuffer, CanFrameBatch, CanFrameFilter};
use logic_types::{
    DecodedBuffer, DecodedEventBatch, DecodedEventFilter, LogicBuffer, LogicSampleBatch,
    LogicSampleFilter,
};
use parking_lot::Mutex;
use std::sync::Arc;
use stream::StreamSource;

/// 带方向与搜索过滤的原始字节流 — 游标增量读取
///
/// 与 RawDataSource 类似, 但只返回方向匹配且包含搜索模式的 chunk。
/// 切换过滤条件时新建本 source, 游标从 collector.base_index 开始,
/// 可自动拉取全部历史匹配数据。
pub struct FilteredRawDataSource {
    collector: Arc<Mutex<RawDataCollector>>,
    read_index: usize,
    direction: DirectionFilter,
    pattern: Option<SearchPattern>,
}

impl FilteredRawDataSource {
    pub fn new(
        collector: Arc<Mutex<RawDataCollector>>,
        direction: DirectionFilter,
        search: Option<&str>,
    ) -> Self {
        let read_index = collector.lock().base_index();
        Self {
            collector,
            read_index,
            direction,
            pattern: search.and_then(SearchPattern::parse),
        }
    }
}

impl StreamSource for FilteredRawDataSource {
    type Batch = RawDataBatch;

    fn backlog(&mut self) -> usize {
        self.collector.lock().remaining_bytes_from(self.read_index)
    }

    fn drain(&mut self, max: usize) -> Option<Self::Batch> {
        let (chunks, next_index) = {
            self.collector.lock().read_filtered_from(
                self.read_index,
                max,
                self.direction,
                self.pattern.as_ref(),
            )
        };
        self.read_index = next_index;
        if chunks.is_empty() {
            None
        } else {
            Some(
                RawDrain {
                    chunks,
                    total_bytes: 0,
                    dropped_bytes: 0,
                }
                .into_batch(),
            )
        }
    }

    fn set_seq(batch: &mut Self::Batch, seq: u64) {
        batch.seq = seq;
    }

    const ACTIVATION_UNIT: usize = 256 * 1024;
    const MAX_DRAIN: usize = 1024 * 1024;
}

/// 带过滤条件的 CAN 帧流 — 游标增量读取
pub struct FilteredCanStreamSource {
    buffer: Arc<Mutex<CanBuffer>>,
    cursor: u64,
    filter: CanFrameFilter,
}

impl FilteredCanStreamSource {
    /// 游标从 0 开始 — drain_from 自动对齐到缓冲区最旧可读位置,
    /// 即可先拉取全部历史匹配帧, 之后严格增量。
    pub const fn new(buffer: Arc<Mutex<CanBuffer>>, filter: CanFrameFilter) -> Self {
        Self {
            buffer,
            cursor: 0,
            filter,
        }
    }
}

impl StreamSource for FilteredCanStreamSource {
    type Batch = CanFrameBatch;

    fn backlog(&mut self) -> usize {
        let buf = self.buffer.lock();
        usize::try_from(buf.version().saturating_sub(self.cursor)).unwrap_or(usize::MAX)
    }

    fn drain(&mut self, max: usize) -> Option<Self::Batch> {
        let buf = self.buffer.lock();
        let (items, new_cursor, _dropped) = buf.drain_from(self.cursor, max);
        self.cursor = new_cursor;
        let frames: Vec<_> = items
            .into_iter()
            .filter(|f| self.filter.matches(f))
            .collect();
        if frames.is_empty() {
            None
        } else {
            Some(CanFrameBatch { seq: 0, frames })
        }
    }

    fn set_seq(batch: &mut Self::Batch, seq: u64) {
        batch.seq = seq;
    }

    const ACTIVATION_UNIT: usize = 1000;
    const MAX_DRAIN: usize = 2000;
}

/// 带过滤条件的逻辑采样流 — 游标增量读取
pub struct FilteredLogicStreamSource {
    buffer: Arc<Mutex<LogicBuffer>>,
    cursor: u64,
    filter: LogicSampleFilter,
}

impl FilteredLogicStreamSource {
    /// 游标从 0 开始 — 自动对齐最旧可读位置, 先拉历史匹配采样, 之后增量
    pub const fn new(buffer: Arc<Mutex<LogicBuffer>>, filter: LogicSampleFilter) -> Self {
        Self {
            buffer,
            cursor: 0,
            filter,
        }
    }
}

impl StreamSource for FilteredLogicStreamSource {
    type Batch = LogicSampleBatch;

    fn backlog(&mut self) -> usize {
        let buf = self.buffer.lock();
        usize::try_from(buf.version().saturating_sub(self.cursor)).unwrap_or(usize::MAX)
    }

    fn drain(&mut self, max: usize) -> Option<Self::Batch> {
        let buf = self.buffer.lock();
        let (items, new_cursor, _dropped) = buf.drain_from(self.cursor, max);
        self.cursor = new_cursor;
        let samples: Vec<_> = items
            .into_iter()
            .filter(|s| self.filter.matches(s))
            .collect();
        if samples.is_empty() {
            None
        } else {
            Some(LogicSampleBatch { seq: 0, samples })
        }
    }

    fn set_seq(batch: &mut Self::Batch, seq: u64) {
        batch.seq = seq;
    }

    const ACTIVATION_UNIT: usize = 2000;
    const MAX_DRAIN: usize = 4000;
}

/// 带过滤条件的解码事件流 — 游标增量读取
pub struct FilteredDecodedStreamSource {
    buffer: Arc<Mutex<DecodedBuffer>>,
    cursor: u64,
    filter: DecodedEventFilter,
}

impl FilteredDecodedStreamSource {
    /// 游标从 0 开始 — 自动对齐最旧可读位置, 先拉历史匹配事件, 之后增量
    pub const fn new(buffer: Arc<Mutex<DecodedBuffer>>, filter: DecodedEventFilter) -> Self {
        Self {
            buffer,
            cursor: 0,
            filter,
        }
    }
}

impl StreamSource for FilteredDecodedStreamSource {
    type Batch = DecodedEventBatch;

    fn backlog(&mut self) -> usize {
        let buf = self.buffer.lock();
        usize::try_from(buf.version().saturating_sub(self.cursor)).unwrap_or(usize::MAX)
    }

    fn drain(&mut self, max: usize) -> Option<Self::Batch> {
        let buf = self.buffer.lock();
        let (items, new_cursor, _dropped) = buf.drain_from(self.cursor, max);
        self.cursor = new_cursor;
        let events: Vec<_> = items
            .into_iter()
            .filter(|e| self.filter.matches(e))
            .collect();
        if events.is_empty() {
            None
        } else {
            Some(DecodedEventBatch { seq: 0, events })
        }
    }

    fn set_seq(batch: &mut Self::Batch, seq: u64) {
        batch.seq = seq;
    }

    const ACTIVATION_UNIT: usize = 500;
    const MAX_DRAIN: usize = 1000;
}
