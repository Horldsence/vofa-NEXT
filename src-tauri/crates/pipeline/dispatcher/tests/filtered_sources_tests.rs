//! 过滤 Source 测试 — 历史回溯 + 增量 + 过滤条件

use std::sync::Arc;

use buffer_raw::{DirectionFilter, RawDataCollector, RawDataDirection};
use can_types::{CanBuffer, CanDirection, CanFrame, CanFrameFilter};
use dispatcher::filtered_sources::{FilteredCanStreamSource, FilteredRawDataSource};
use parking_lot::Mutex;
use stream::StreamSource;

fn can_frame(id: u32, direction: CanDirection) -> CanFrame {
    CanFrame {
        timestamp: u64::from(id),
        id,
        extended: false,
        rtr: false,
        dlc: 1,
        data: vec![u8::try_from(id).unwrap_or(0)],
        direction,
    }
}

const fn rx_only_filter() -> CanFrameFilter {
    CanFrameFilter {
        rx_only: true,
        tx_only: false,
        id_whitelist: vec![],
        id_blacklist: vec![],
    }
}

/// 新建 Source (游标 0) 自动对齐到最旧可读位置 — 先拉全部历史匹配帧
#[test]
fn filtered_can_source_pulls_matching_history_first() {
    let buffer = Arc::new(Mutex::new(CanBuffer::new(100)));
    for id in [1, 2, 3] {
        let direction = if id == 2 {
            CanDirection::Tx
        } else {
            CanDirection::Rx
        };
        buffer.lock().push(can_frame(id, direction));
    }
    let mut source = FilteredCanStreamSource::new(buffer.clone(), rx_only_filter());

    assert!(source.backlog() > 0, "游标 0 → backlog 按版本差计");
    let batch = source.drain(100).expect("历史匹配帧应产出");
    assert_eq!(
        batch.frames.iter().map(|f| f.id).collect::<Vec<_>>(),
        vec![1, 3],
        "Tx 帧 (id=2) 被过滤"
    );

    // 之后严格增量: 新匹配帧可见
    buffer.lock().push(can_frame(4, CanDirection::Rx));
    buffer.lock().push(can_frame(5, CanDirection::Tx));
    let batch = source.drain(100).expect("新帧应产出");
    assert_eq!(
        batch.frames.iter().map(|f| f.id).collect::<Vec<_>>(),
        vec![4]
    );
}

/// 白名单过滤: 全部不匹配 → drain 返回 None
#[test]
fn filtered_can_source_returns_none_when_nothing_matches() {
    let buffer = Arc::new(Mutex::new(CanBuffer::new(100)));
    buffer.lock().push(can_frame(1, CanDirection::Rx));
    buffer.lock().push(can_frame(2, CanDirection::Rx));
    let filter = CanFrameFilter {
        rx_only: false,
        tx_only: false,
        id_whitelist: vec![99],
        id_blacklist: vec![],
    };
    let mut source = FilteredCanStreamSource::new(buffer, filter);
    assert!(source.drain(100).is_none(), "白名单无命中应返回 None");
}

/// 原始字节流过滤: 方向 + 搜索模式组合, 混合 chunk 中只留匹配者
#[test]
fn filtered_raw_source_applies_direction_and_search() {
    let collector = Arc::new(Mutex::new(RawDataCollector::new()));
    {
        let mut c = collector.lock();
        c.push_chunk(1, RawDataDirection::Rx, b"hello");
        c.push_chunk(2, RawDataDirection::Tx, b"ERR-tx");
        c.push_chunk(3, RawDataDirection::Rx, b"ERR-rx");
    }

    // 仅方向过滤: Rx 命中 2 条
    let mut rx = FilteredRawDataSource::new(collector.clone(), DirectionFilter::Rx, None);
    let batch = rx.drain(1024).expect("Rx 方向应有匹配");
    assert_eq!(batch.chunks.len(), 2, "Tx chunk 被过滤");
    assert!(rx.drain(1024).is_none());

    // 方向 + 搜索组合: Rx 且含 "ERR" → 仅 1 条
    let mut rx_err =
        FilteredRawDataSource::new(collector.clone(), DirectionFilter::Rx, Some("ERR"));
    let batch = rx_err.drain(1024).expect("ERR-rx 应命中");
    assert_eq!(batch.chunks.len(), 1, "方向+搜索双条件");
    assert_eq!(batch.chunks[0].timestamp_us, 3);

    // 搜索模式按 UTF-8 文本解析 (非 hex): "zz" 不在任何 chunk 中
    let mut miss = FilteredRawDataSource::new(collector, DirectionFilter::All, Some("zz"));
    assert!(miss.drain(1024).is_none(), "搜索无命中应返回 None");
}

/// 无过滤条件时行为与基础 RawDataSource 一致 (全部 chunk 可见)
#[test]
fn unfiltered_raw_source_sees_everything() {
    let collector = Arc::new(Mutex::new(RawDataCollector::new()));
    collector
        .lock()
        .push_chunk(1, RawDataDirection::Tx, b"tx-only");
    let mut source = FilteredRawDataSource::new(collector, DirectionFilter::All, None);
    let batch = source.drain(64).expect("无过滤应全量可见");
    assert_eq!(batch.chunks.len(), 1);
    assert!(!batch.chunks[0].bytes_b64.is_empty());
}
