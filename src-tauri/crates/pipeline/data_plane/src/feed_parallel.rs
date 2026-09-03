//! # feed_parallel — feed (RX 解析) 段自动并行
//!
//! 与 send 侧 stream.rs 分片推送相同的自动并发语义:
//!
//! - **积压低**: 单 worker 顺序解析 (调用方走原有 `p.feed(&data)` 路径, 零变化)。
//! - **积压高**: 自动扩展至最多 [`MAX_FEED_WORKERS`] 个并行 worker,
//!   按帧边界 ([`ProtocolEngine::split_aligned`]) 切开批次, 空状态引擎
//!   ([`ProtocolEngine::new_worker`]) 独立解析每块, 结果按块序合并 — 与顺序解析等价。
//! - **积压消退**: 自动回落顺序模式, 不完整尾字节 ([`ParallelFeeder::take_pending`])
//!   喂回主引擎内部缓冲, 零丢失。
//!
//! 仅帧定界协议 (JustFloat / FireWater / Slcan / CandleLight) 支持;
//! LogicDecoder (跨字节状态机) / RawData (无帧) 由 `split_aligned` 返回 None 回退。

use parking_lot::Mutex;
use protocol_engine::{FeedOutput, ProtocolEngine};
use std::sync::Arc;
use vofa_core::PipelineConfig;

/// 最大并行 worker 数 — 默认安全上限, 实际运行由 PipelineConfig::max_workers 提供
/// (常量保留为默认值文档来源, 与 PipelineConfig::default() 保持同步, 见下方单测)
#[allow(dead_code)]
pub const MAX_FEED_WORKERS: usize = 8;
/// 积压单位: parse mpsc 每 8 批升一级 — 默认值, 实际由 PipelineConfig::feed_parallel_unit 提供
#[allow(dead_code)]
pub const FEED_PARALLEL_UNIT: usize = 8;
/// 每 worker 至少摊到 32KB 才值得并行 — 默认值, 实际由 PipelineConfig::min_worker_bytes_kb 提供
#[allow(dead_code)]
pub const MIN_WORKER_BYTES: usize = 32 * 1024;

fn worker_limit(cfg: &PipelineConfig) -> usize {
    cfg.max_workers
        .min(std::thread::available_parallelism().map_or(1, std::num::NonZero::get))
        .max(1)
}

/// 按积压深度与批次字节数计算需要的 worker 数 (1..=cfg.max_workers)。
pub fn workers_needed(depth: usize, bytes: usize, cfg: &PipelineConfig) -> usize {
    (1 + depth / FEED_PARALLEL_UNIT)
        .min(1 + bytes / MIN_WORKER_BYTES)
        .min(worker_limit(cfg))
}

/// 并行解析细分耗时 (观测用, 不影响行为)
#[derive(Default, Clone, Copy)]
pub struct ParallelTiming {
    /// split_aligned + 锁内准备耗时
    pub split_ns: u64,
    /// spawn_blocking + 按序 join 合并耗时
    pub join_ns: u64,
}

/// 并行解析编排器 — 常驻 worker 池 + 跨批次不完整尾字节缓冲
pub struct ParallelFeeder {
    /// 常驻复用的 worker 池 (懒建, 按需补足到切分块数)
    workers: Vec<Box<dyn ProtocolEngine>>,
    /// 上一批的不完整帧尾字节 (下一批前置拼接)
    pub(crate) pending: Vec<u8>,
}

impl ParallelFeeder {
    pub fn new() -> Self {
        Self {
            workers: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// 并行解析一批数据
    ///
    /// 返回 (合并后的 FeedOutput, 自动通道检测结果, 细分耗时)。
    /// detection 仅自动模式下取第一块对应 worker 解析后的 detected_channels(),
    /// 是否通知由调用方 (detection_notified 一次性逻辑) 决定。
    ///
    /// 计时口径: split_ns 含 pending 前置拼接与锁内 split+建池; join_ns 含
    /// 尾部保存、spawn_blocking 与按序 join 合并。
    ///
    /// 调用方需保证主引擎 split_aligned 支持并行 (否则完整批次会滞留在 pending)。
    pub async fn feed(
        &mut self,
        proto: &Arc<Mutex<Box<dyn ProtocolEngine>>>,
        data: &[u8],
        workers: usize,
    ) -> (FeedOutput, Option<usize>, ParallelTiming) {
        let mut timing = ParallelTiming::default();
        let t_split = std::time::Instant::now();
        // 1. pending 前置拼接 (上一批的不完整帧尾)
        let mut full = std::mem::take(&mut self.pending);
        full.extend_from_slice(data);

        // 2. 锁内计算切分 + 懒建 worker 池 (按需补足到块数) + 读取自动模式标记
        let (ranges, auto_mode) = {
            let p = proto.lock();
            let Some(ranges) = p.split_aligned(&full, workers) else {
                // 协议不支持并行 — 防御性回退: 整批留待下次, 调用方不应进入此路径
                self.pending = full;
                timing.split_ns = u64::try_from(t_split.elapsed().as_nanos()).unwrap_or(u64::MAX);
                return (FeedOutput::default(), None, timing);
            };
            let missing = ranges.len().saturating_sub(self.workers.len());
            for _ in 0..missing {
                self.workers.push(p.new_worker());
            }
            (ranges, p.is_auto_mode())
        };
        timing.split_ns = u64::try_from(t_split.elapsed().as_nanos()).unwrap_or(u64::MAX);

        let t_join = std::time::Instant::now();
        // 3. 尾部不完整帧存入 pending, 下一批前置拼接
        let tail_start = ranges.last().map_or(0, |r| r.end);
        self.pending = full[tail_start..].to_vec();

        if ranges.is_empty() {
            timing.join_ns = u64::try_from(t_join.elapsed().as_nanos()).unwrap_or(u64::MAX);
            return (FeedOutput::default(), None, timing);
        }

        // 4. 每块 copy 后与池中 worker 配对, spawn_blocking 并行解析
        let workers: Vec<_> = self.workers.drain(..ranges.len()).collect();
        let mut handles = Vec::with_capacity(ranges.len());
        for (range, mut worker) in ranges.iter().zip(workers) {
            let chunk = full[range.clone()].to_vec();
            handles.push(tokio::task::spawn_blocking(move || {
                let out = worker.feed(&chunk);
                (worker, out)
            }));
        }

        // 5. 按块序收回 worker 入池并合并输出 (下游顺序与顺序解析一致)
        let mut merged = FeedOutput::default();
        let mut detection = None;
        let mut first = true;
        for h in handles {
            match h.await {
                Ok((worker, out)) => {
                    // 自动通道检测: 取第一块对应 worker 的解析结果
                    if first && auto_mode {
                        detection = worker.detected_channels();
                    }
                    first = false;
                    self.workers.push(worker);
                    merged.append(out);
                }
                Err(e) => {
                    log::warn!("并行解析 worker 任务失败: {e}");
                }
            }
        }
        timing.join_ns = u64::try_from(t_join.elapsed().as_nanos()).unwrap_or(u64::MAX);
        (merged, detection, timing)
    }

    /// 取出 pending 中的不完整字节 (回落顺序模式时喂回主引擎)
    pub fn take_pending(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }
}

impl Default for ParallelFeeder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_match_pipeline_config_default() {
        // 模块常量与 PipelineConfig::default() 保持同步 (常量是默认值文档来源)
        let d = PipelineConfig::default();
        assert_eq!(d.max_workers, MAX_FEED_WORKERS);
    }

    #[test]
    fn test_workers_needed_idle() {
        let cfg = PipelineConfig::default();
        // 无积压 → 单 worker
        assert_eq!(workers_needed(0, 0, &cfg), 1);
        assert_eq!(workers_needed(7, 0, &cfg), 1);
        // 字节数大但无积压 → 单 worker
        assert_eq!(workers_needed(0, 64 * 1024, &cfg), 1);
        // 积压高但批次小 → 单 worker
        assert_eq!(workers_needed(255, 1024, &cfg), 1);
    }

    #[test]
    fn test_workers_needed_scaling() {
        let cfg = PipelineConfig::default();
        let big = 256 * 1024;
        // 每 8 批积压升一级
        assert_eq!(workers_needed(8, big, &cfg), 2);
        assert_eq!(workers_needed(15, big, &cfg), 2);
        assert_eq!(workers_needed(16, big, &cfg), 3);
        assert_eq!(workers_needed(24, big, &cfg), 4);
    }

    #[test]
    fn test_workers_needed_clamp() {
        let cfg = PipelineConfig::default();
        let big = 1024 * 1024;
        let max_workers = worker_limit(&cfg);
        // 上限 clamp 到 max_workers
        assert_eq!(workers_needed(255, big, &cfg), max_workers);
        assert_eq!(workers_needed(1000, big, &cfg), max_workers);
        // 字节数下限: 32KB 才够 2 worker
        assert_eq!(workers_needed(8, MIN_WORKER_BYTES - 1, &cfg), 1);
        assert_eq!(workers_needed(8, MIN_WORKER_BYTES, &cfg), 2);
    }

    #[test]
    fn test_workers_needed_custom_config() {
        // 自定义安全上限
        let cfg = PipelineConfig {
            max_workers: 2,
            ..PipelineConfig::default()
        };
        // 8 批积压 + 32KB → 2 worker
        assert_eq!(workers_needed(8, 32 * 1024, &cfg), 2);
        // 积压再高也被 clamp 到 2
        assert_eq!(workers_needed(255, 1024 * 1024, &cfg), 2);
        // 3 批积压 (< 4) → 单 worker
        assert_eq!(workers_needed(7, 1024 * 1024, &cfg), 1);
    }
}
