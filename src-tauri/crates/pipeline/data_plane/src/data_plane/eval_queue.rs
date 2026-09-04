//! 每源评估积压预算：限制帧、分配容量和批数，吸收短时调度抖动而不无限积压。

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use vofa_core::DataFrame;

// 700k/s 四通道下最多约 200ms；不是所有采样率下的固定延迟保证。
const MAX_FRAMES: usize = 140_000;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_BATCHES: usize = 256;
/// 只合并已经到达的小批，不等待填满；限制单次求值的占用与跨源轮询粒度。
const COALESCED_FRAMES: usize = 16_384;

pub struct QueuedBatch {
    pub frames: Arc<Vec<DataFrame>>,
    pub enqueued: Instant,
    bytes: usize,
}

#[derive(Default)]
pub struct FrameQueue {
    batches: VecDeque<QueuedBatch>,
    /// 缺口归属于下一次出队批次，不能被已经出队的较早批次消费。
    pending_gap: bool,
    pub frames: usize,
    /// Vec 分配容量的估计，不包含 allocator 元数据、执行中批次或其他源。
    pub bytes: usize,
}

impl FrameQueue {
    pub fn len(&self) -> usize {
        self.batches.len()
    }

    pub fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }

    /// 满时逐批丢最旧；单批自身超预算时整批拒绝，并清空更旧的积压。
    /// 原始记录已独立入库；返回所有未求值帧数供显式缺口记账。
    pub fn push(&mut self, frames: Arc<Vec<DataFrame>>) -> u64 {
        if frames.is_empty() {
            return 0;
        }
        let bytes = frames.iter().fold(
            frames.capacity().saturating_mul(size_of::<DataFrame>()),
            |sum, frame| {
                sum.saturating_add(frame.channels.capacity().saturating_mul(size_of::<f32>()))
            },
        );
        if frames.len() > MAX_FRAMES || bytes > MAX_BYTES {
            self.pending_gap = true;
            return self.clear().saturating_add(frames.len() as u64);
        }
        let mut dropped = 0;
        while self.len() >= MAX_BATCHES
            || self.frames + frames.len() > MAX_FRAMES
            || self.bytes + bytes > MAX_BYTES
        {
            if let Some(old) = self.pop() {
                dropped += old.frames.len() as u64;
            }
        }
        self.frames += frames.len();
        self.bytes += bytes;
        self.pending_gap |= dropped > 0;
        self.batches.push_back(QueuedBatch {
            frames,
            enqueued: Instant::now(),
            bytes,
        });
        dropped
    }

    pub fn pop(&mut self) -> Option<QueuedBatch> {
        let batch = self.batches.pop_front()?;
        self.frames -= batch.frames.len();
        self.bytes -= batch.bytes;
        Some(batch)
    }

    /// 积压恢复时摊薄任务提交和线程创建成本。仅移动独占批中的 DataFrame，
    /// 通道 Vec 的底层分配保持不变；共享 Arc 不做深拷贝，也不越过它乱序消费。
    pub fn pop_ready(&mut self) -> Option<(Arc<Vec<DataFrame>>, Instant, bool)> {
        let mut first = self.pop()?;
        if let Some(frames) = Arc::get_mut(&mut first.frames) {
            while let Some(next) = self.batches.front_mut() {
                if frames.len().saturating_add(next.frames.len()) > COALESCED_FRAMES {
                    break;
                }
                let Some(next_frames) = Arc::get_mut(&mut next.frames) else {
                    break;
                };
                let count = next_frames.len();
                frames.append(next_frames);
                let consumed = self.batches.pop_front().expect("批头已检查");
                self.frames -= count;
                self.bytes -= consumed.bytes;
            }
        }
        Some((
            first.frames,
            first.enqueued,
            std::mem::take(&mut self.pending_gap),
        ))
    }

    pub fn clear(&mut self) -> u64 {
        let dropped = self.frames as u64;
        self.batches.clear();
        self.frames = 0;
        self.bytes = 0;
        dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(count: usize, channels: usize, timestamp: u64) -> Arc<Vec<DataFrame>> {
        Arc::new(
            (0..count)
                .map(|_| DataFrame {
                    timestamp,
                    channels: vec![0.0; channels],
                })
                .collect(),
        )
    }

    #[test]
    fn frame_budget_keeps_latest_and_recovers_accounting() {
        let mut queue = FrameQueue::default();
        for i in 0..40 {
            assert_eq!(queue.push(batch(3_500, 4, i)), 0);
        }
        assert_eq!(queue.frames, MAX_FRAMES);
        assert_eq!(queue.push(batch(3_500, 4, 40)), 3_500);
        assert_eq!(queue.pop().unwrap().frames[0].timestamp, 1);
        assert_eq!(queue.clear(), 39 * 3_500);
        assert_eq!((queue.frames, queue.bytes, queue.len()), (0, 0, 0));
    }

    #[test]
    fn wide_channels_and_oversized_batches_respect_byte_budget() {
        let mut queue = FrameQueue::default();
        assert_eq!(queue.push(batch(1_000, 1_024, 0)), 0);
        assert_eq!(queue.push(batch(1_000, 1_024, 1)), 0);
        assert_eq!(queue.push(batch(1_000, 1_024, 2)), 1_000);
        assert!(queue.bytes <= MAX_BYTES);
        assert_eq!(queue.push(batch(3_000, 1_024, 3)), 5_000);
        assert!(queue.is_empty());
        assert_eq!((queue.frames, queue.bytes), (0, 0));
        assert_eq!(
            queue.push(batch(MAX_FRAMES + 1, 0, 4)),
            (MAX_FRAMES + 1) as u64
        );
        assert!(queue.is_empty());
    }

    #[test]
    fn tiny_batches_have_bounded_metadata() {
        let mut queue = FrameQueue::default();
        for i in 0..MAX_BATCHES {
            assert_eq!(queue.push(batch(1, 1, i as u64)), 0);
        }
        assert_eq!(queue.push(batch(1, 1, 256)), 1);
        assert_eq!(queue.len(), MAX_BATCHES);
        assert_eq!(queue.pop().unwrap().frames[0].timestamp, 1);
    }

    #[test]
    fn queued_small_batches_merge_in_order_without_copying_channels() {
        let mut queue = FrameQueue::default();
        let first = batch(3_500, 4, 1);
        let channels_ptr = first[0].channels.as_ptr();
        queue.push(first);
        for i in 2..=5 {
            queue.push(batch(3_500, 4, i));
        }
        let (merged, _, _) = queue.pop_ready().unwrap();
        assert_eq!(merged.len(), 14_000);
        assert_eq!(merged[0].channels.as_ptr(), channels_ptr);
        for (i, chunk) in merged.chunks_exact(3_500).enumerate() {
            assert!(chunk.iter().all(|frame| frame.timestamp == i as u64 + 1));
        }
        assert_eq!(queue.frames, 3_500);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.pop_ready().unwrap().0[0].timestamp, 5);
        assert_eq!((queue.frames, queue.bytes), (0, 0));
    }

    #[test]
    fn shared_batches_are_not_cloned_mutated_or_reordered() {
        let mut queue = FrameQueue::default();
        let shared = batch(1, 4, 2);
        queue.push(batch(1, 4, 1));
        queue.push(shared.clone());
        queue.push(batch(1, 4, 3));
        assert_eq!(queue.pop_ready().unwrap().0.len(), 1);
        let (second, _, _) = queue.pop_ready().unwrap();
        assert!(Arc::ptr_eq(&second, &shared));
        assert_eq!(shared.len(), 1);
        assert_eq!(queue.pop_ready().unwrap().0[0].timestamp, 3);
        assert_eq!((queue.frames, queue.bytes), (0, 0));
    }

    #[test]
    fn gap_belongs_to_next_retained_batch_not_an_in_flight_batch() {
        let mut queue = FrameQueue::default();
        queue.push(batch(1, 1, 0));
        let (_, _, earlier_gap) = queue.pop_ready().unwrap();
        for i in 1..=257 {
            queue.push(batch(1, 1, i));
        }
        assert!(!earlier_gap);
        let (retained, _, gap) = queue.pop_ready().unwrap();
        assert_eq!(retained[0].timestamp, 2);
        assert!(gap);
        queue.push(batch(1, 1, 258));
        assert!(!queue.pop_ready().unwrap().2);

        // 拒绝的超大批没有可出队项，缺口必须保留到后续可接受批到达。
        queue.push(batch(MAX_FRAMES + 1, 0, 259));
        assert!(queue.pop_ready().is_none());
        queue.push(batch(1, 1, 260));
        assert!(queue.pop_ready().unwrap().2);
    }
}
