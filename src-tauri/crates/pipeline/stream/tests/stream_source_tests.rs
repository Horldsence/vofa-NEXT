//! 统一分片流框架测试 — bounded_drain_size / 分片组状态机 / AdaptiveRate / 游标源

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use buffer_databuffer::DataBuffer;
use buffer_raw::{RawDataCollector, RawDataDirection};
use can_types::{CanBuffer, CanDirection, CanFrame};
use parking_lot::Mutex;
use stream::dispatcher::AdaptiveRate;
use stream::{
    bounded_drain_size, join_or_create_group, leave_group, CanStreamSource, RawDataSource,
    StreamSource, WaveformSource,
};

// ---- bounded_drain_size (自 cmd/display 借测归位) ----

#[test]
fn detail_budget_above_source_limit_is_safely_capped() {
    assert_eq!(bounded_drain_size(1, 8_000, 5_000), 5_000);
    assert_eq!(bounded_drain_size(9_000, 1_000, 5_000), 5_000);
    assert_eq!(bounded_drain_size(2_500, 1_000, 5_000), 2_500);
}

/// min_batch 超过 max_drain 时 usize::clamp 会 panic — 必须先收紧最小值
#[test]
fn min_batch_above_max_drain_never_panics() {
    assert_eq!(bounded_drain_size(0, 8_000, 5_000), 5_000);
    assert_eq!(bounded_drain_size(100, 12_000, 5_000), 5_000);
}

#[test]
fn zero_backlog_returns_tightened_min_batch() {
    assert_eq!(bounded_drain_size(0, 1_000, 5_000), 1_000);
}

#[test]
fn waveform_source_accepts_full_detail_budget() {
    assert_eq!(<WaveformSource as StreamSource>::MAX_DRAIN, 12_000);
}

// ---- 分片组状态机 ----

/// 可计数测试源 — 分片组测试用
#[derive(Default, Debug)]
struct CountingSource {
    backlog: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
struct CountingBatch {
    seq: u64,
    items: Vec<u32>,
}

impl StreamSource for CountingSource {
    type Batch = CountingBatch;

    fn backlog(&mut self) -> usize {
        self.backlog
    }

    fn drain(&mut self, max: usize) -> Option<Self::Batch> {
        if self.backlog == 0 {
            return None;
        }
        let n = max.min(self.backlog);
        self.backlog -= n;
        Some(CountingBatch {
            seq: 0,
            items: vec![0; n],
        })
    }

    fn set_seq(batch: &mut Self::Batch, seq: u64) {
        batch.seq = seq;
    }

    const ACTIVATION_UNIT: usize = 10;
    const MAX_DRAIN: usize = 100;
}

fn group_map() -> Arc<Mutex<std::collections::HashMap<String, data_plane::StreamGroupState>>> {
    Arc::new(Mutex::new(std::collections::HashMap::new()))
}

#[test]
fn first_channel_creates_group_as_shard_zero() {
    let groups = group_map();
    let (source, seq, shard, key) =
        join_or_create_group::<CountingSource, _>(&groups, None, 7, 4, CountingSource::default)
            .expect("首个 channel 应建组成功");
    assert_eq!(shard, 0, "首个 channel 是 shard 0");
    assert_eq!(key, "7", "组 id = 首个 channel id");
    assert_eq!(seq.load(Ordering::Relaxed), 0);
    assert_eq!(groups.lock().get("7").map(|g| g.shards), Some(1));
    drop(source);
}

#[test]
fn joining_channels_share_source_and_increment_shards() {
    let groups = group_map();
    let (s0, seq0, _shard0, key) =
        join_or_create_group::<CountingSource, _>(&groups, None, 1, 4, CountingSource::default)
            .expect("建组");
    let (s1, seq1, shard1, _) = join_or_create_group::<CountingSource, _>(
        &groups,
        Some(key.clone()),
        2,
        4,
        CountingSource::default,
    )
    .expect("第二个 channel 加入");
    assert_eq!(shard1, 1);
    assert_eq!(groups.lock().get(&key).map(|g| g.shards), Some(2));
    // 组内共享同一流源实例与组级 seq
    assert!(Arc::ptr_eq(&s0, &s1), "组员应共享流源实例");
    assert!(Arc::ptr_eq(&seq0, &seq1), "组员应共享组级 seq");
}

#[test]
fn full_group_rejects_new_shards() {
    let groups = group_map();
    let (_, _, _, key) =
        join_or_create_group::<CountingSource, _>(&groups, None, 1, 2, CountingSource::default)
            .expect("建组");
    join_or_create_group::<CountingSource, _>(
        &groups,
        Some(key.clone()),
        2,
        2,
        CountingSource::default,
    )
    .expect("第二个分片应成功");
    let err = join_or_create_group::<CountingSource, _>(
        &groups,
        Some(key),
        3,
        2,
        CountingSource::default,
    )
    .expect_err("组满应拒绝");
    assert!(err.to_string().contains("分片"), "应报告组满: {err}");
}

#[test]
fn unknown_group_key_reports_not_found() {
    let groups = group_map();
    let err = join_or_create_group::<CountingSource, _>(
        &groups,
        Some("ghost".into()),
        1,
        4,
        CountingSource::default,
    )
    .expect_err("组不存在应报错");
    assert!(
        err.to_string().contains("不存在") || err.to_string().contains("not found"),
        "应报告组不存在: {err}"
    );
}

#[test]
fn type_mismatch_reports_downcast_error() {
    let groups = group_map();
    let (_, _, _, key) =
        join_or_create_group::<CountingSource, _>(&groups, None, 1, 4, CountingSource::default)
            .expect("以 CountingSource 建组");
    // 另一类型加入同一组 — downcast 失败 (OtherSource 定义在模块级)
    let err = join_or_create_group::<OtherSource, _>(&groups, Some(key), 2, 4, || OtherSource)
        .expect_err("类型不符应报错");
    assert!(err.to_string().contains("类型"), "应报告类型不匹配: {err}");
}

#[test]
fn leave_group_removes_empty_group_only() {
    let groups = group_map();
    let (_, _, _, key) =
        join_or_create_group::<CountingSource, _>(&groups, None, 1, 4, CountingSource::default)
            .expect("建组");
    join_or_create_group::<CountingSource, _>(
        &groups,
        Some(key.clone()),
        2,
        4,
        CountingSource::default,
    )
    .expect("加入");

    leave_group(&groups, &key);
    assert_eq!(
        groups.lock().get(&key).map(|g| g.shards),
        Some(1),
        "退出后剩 1 分片"
    );

    leave_group(&groups, &key);
    assert!(!groups.lock().contains_key(&key), "空组应被移除");

    // 对不存在的组 leave 是安全 no-op
    leave_group(&groups, &key);
}

/// 类型不符测试用的异类流源
#[derive(Debug)]
struct OtherSource;

impl StreamSource for OtherSource {
    type Batch = CountingBatch;
    fn backlog(&mut self) -> usize {
        0
    }
    fn drain(&mut self, _max: usize) -> Option<Self::Batch> {
        None
    }
    fn set_seq(_batch: &mut Self::Batch, _seq: u64) {}
    const ACTIVATION_UNIT: usize = 1;
    const MAX_DRAIN: usize = 1;
}

// ---- AdaptiveRate ----

#[test]
fn adaptive_rate_starts_at_min_and_send_converges_to_min() {
    let mut rate = AdaptiveRate::new(Duration::from_millis(16), Duration::from_millis(100));
    assert_eq!(rate.current(), Duration::from_millis(16));
    rate.on_idle();
    rate.on_send();
    assert_eq!(rate.current(), Duration::from_millis(16), "send 后回到 min");
}

#[test]
fn adaptive_rate_idle_backs_off_to_max_cap() {
    let mut rate = AdaptiveRate::new(Duration::from_millis(16), Duration::from_millis(100));
    for _ in 0..20 {
        rate.on_idle();
    }
    assert_eq!(
        rate.current(),
        Duration::from_millis(100),
        "空闲退避封顶 max"
    );
    rate.on_send();
    assert_eq!(rate.current(), Duration::from_millis(50), "send 减半");
}

/// 概览 100ms 的请求间隔是发送下限, 不被 on_send 打穿 (display_rate 的前提)
#[test]
fn adaptive_rate_never_goes_below_min() {
    let mut rate = AdaptiveRate::new(Duration::from_millis(100), Duration::from_millis(100));
    for _ in 0..10 {
        rate.on_send();
    }
    assert_eq!(rate.current(), Duration::from_millis(100));
}

// ---- RawDataSource (游标增量) ----

#[test]
fn raw_source_drains_pushed_chunks_in_order_then_none() {
    let collector = Arc::new(Mutex::new(RawDataCollector::new()));
    collector
        .lock()
        .push_chunk(1, RawDataDirection::Rx, b"hello");
    collector
        .lock()
        .push_chunk(2, RawDataDirection::Rx, b"world");
    let mut source = RawDataSource::new(collector);

    assert_eq!(source.backlog(), 10);
    let batch = source.drain(1024).expect("有数据应产出批次");
    assert_eq!(batch.chunks.len(), 2);
    assert_eq!(batch.chunks[0].timestamp_us, 1);
    assert_eq!(batch.chunks[1].timestamp_us, 2);
    assert_eq!(batch.chunks[0].direction, RawDataDirection::Rx);
    assert!(source.drain(1024).is_none(), "排空后应返回 None");
    assert_eq!(source.backlog(), 0);
}

/// 游标落后于 collector.base_index (数据被容量覆盖丢弃) 时自动对齐, 不 panic 不重复
#[test]
fn raw_source_cursor_behind_base_index_auto_aligns() {
    let collector = Arc::new(Mutex::new(RawDataCollector::new()));
    let mut source = RawDataSource::new(Arc::clone(&collector));
    assert!(source.drain(16).is_none(), "空 collector 无数据");
    // 建源后继续写入 — 游标从建源时刻起 (自动对齐语义的前半部分)
    collector.lock().push_chunk(1, RawDataDirection::Rx, b"ab");
    let batch = source.drain(16).expect("新写入数据可见");
    assert_eq!(batch.chunks.len(), 1);
    assert!(source.drain(16).is_none());
    // 继续写入对新订阅者可见
    collector.lock().push_chunk(2, RawDataDirection::Rx, b"xyz");
    let batch = source.drain(16).expect("新写入数据可见");
    assert_eq!(batch.chunks.len(), 1);
    assert_eq!(batch.chunks[0].timestamp_us, 2);
    assert_eq!(source.backlog(), 0);
}

// ---- CanStreamSource (游标起点回溯 max_items) ----

fn can_frame(id: u32) -> CanFrame {
    CanFrame {
        timestamp: u64::from(id),
        id,
        extended: false,
        rtr: false,
        dlc: 1,
        data: vec![u8::try_from(id).unwrap_or(0)],
        direction: CanDirection::Rx,
    }
}

#[test]
fn can_source_cursor_backtracks_max_items_for_history() {
    let buffer = Arc::new(Mutex::new(CanBuffer::new(100)));
    for id in 1..=5u32 {
        buffer.lock().push(can_frame(id));
    }
    let mut source = CanStreamSource::new(buffer, 3);
    assert_eq!(source.backlog(), 3, "游标回溯 max_items=3");
    let batch = source.drain(10).expect("有历史应产出");
    assert_eq!(
        batch.frames.iter().map(|f| f.id).collect::<Vec<_>>(),
        vec![3, 4, 5],
        "订阅即见最近 3 帧 (游标回溯 max_items)"
    );
    assert_eq!(source.backlog(), 0, "已追平");
    // 追平后再 drain: 按契约返回当前缓冲快照 (cursor >= version 语义), 不再前进
    let catch_up = source.drain(10).expect("追平语义: 返回当前缓冲");
    assert_eq!(catch_up.frames.len(), 5);
}

#[test]
fn can_source_sees_frames_pushed_after_subscribe() {
    let buffer = Arc::new(Mutex::new(CanBuffer::new(100)));
    let mut source = CanStreamSource::new(buffer.clone(), 10);
    assert!(source.drain(10).is_none());
    buffer.lock().push(can_frame(42));
    let batch = source.drain(10).expect("订阅后新帧可见");
    assert_eq!(batch.frames[0].id, 42);
    assert_eq!(batch.seq, 0, "seq 由 set_seq 在源锁内写入");
}

// ---- WaveformSource (快照语义) ----

#[test]
fn waveform_source_pushes_only_on_version_change() {
    let buffer = Arc::new(Mutex::new(DataBuffer::new(1000, 1)));
    let mut source = WaveformSource::new(buffer.clone());
    assert!(source.drain(500).is_none(), "version 未变化不推送");
    buffer.lock().push_frame_at(1, &[1.0]);
    let window = source.drain(500).expect("version 变化应推送");
    assert_eq!(window.timestamps.len(), 1);
    assert_eq!(window.channels[0], vec![1.0]);
    assert!(source.drain(500).is_none(), "同一 version 不重复推送");
    const { assert!(<WaveformSource as StreamSource>::SNAPSHOT, "波形是快照流") };
}

#[test]
fn waveform_source_with_view_limits_window() {
    let buffer = Arc::new(Mutex::new(DataBuffer::new(1000, 1)));
    {
        let mut b = buffer.lock();
        for i in 0..10u64 {
            b.push_frame_at(i * 1000, &[f32::from(u16::try_from(i).unwrap_or(0))]);
        }
    }
    let mut source = WaveformSource::with_view(
        buffer,
        stream::WaveformViewSpec {
            start_ms: -3.0,
            end_ms: 0.0,
            selection: buffer_databuffer::WaveformSeriesSelection::default(),
        },
    );
    let window = source.drain(500).expect("有视图也应推送");
    // 最近 3ms 窗口 (含起点边界): 6ms/7ms/8ms/9ms 四个点
    assert_eq!(window.timestamps.len(), 4, "视图窗口应限制点数");
}
