//! 流水线诊断指标 — 2s 日志窗口计数 + 生命周期累计

use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// 诊断指标输出间隔
pub const METRICS_REPORT_INTERVAL: Duration = Duration::from_secs(2);

/// 非破坏性评估诊断；最大延迟是自创建以来的墙钟值，不随日志窗口清零。
#[derive(Debug, Clone, Copy)]
pub struct EvalDiagnostics {
    pub queued_batches: usize,
    pub queued_frames: usize,
    /// 所有源待求值 Vec 的分配容量估计，不含执行中批次与 allocator 元数据。
    pub queued_estimated_bytes: usize,
    /// 入队到出队的最大等待纳秒数。
    pub queue_wait_max_ns: u64,
    /// 提交 blocking 任务到该闭包开始执行的最大等待纳秒数。
    pub dispatch_wait_max_ns: u64,
    /// blocking 闭包内取缓冲、配置与求值的最大服务纳秒数，含锁等待。
    pub eval_service_max_ns: u64,
    pub completed_frames: u64,
    pub completed_batches: u64,
    pub dropped_frames: u64,
}

/// 日志窗口计数与独立的进程生命周期累计计数。
#[derive(Default)]
pub struct DataPlaneMetrics {
    /// 收到的消息数 (按广播消息逐条计数) / 字节数 (合批后)
    pub(super) rx_msgs: AtomicU64,
    pub(super) rx_bytes: AtomicU64,
    /// broadcast Lagged 丢弃的消息数
    pub(super) lagged: AtomicU64,
    /// 字节路由累计耗时 ns (协议解析 + 原始记录，不含异步求值) / 批次数
    pub(super) feed_ns: AtomicU64,
    pub(super) feed_batches: AtomicU64,
    /// 记录平面实际接收并入库的协议帧数（去重代表源口径）
    pub(super) frames_ingested: AtomicU64,
    /// 数值平面累计耗时 / 批数 / 消费帧数，与摄入平面独立统计
    pub(super) eval_ns: AtomicU64,
    pub(super) eval_batches: AtomicU64,
    pub(super) frames_evaled: AtomicU64,
    /// 评估队列溢出丢弃的帧数 (摄入/评估解耦后的显式降级计数)
    pub(super) eval_dropped: AtomicU64,
    pub(super) eval_completed_total: AtomicU64,
    pub(super) eval_batches_total: AtomicU64,
    pub(super) eval_dropped_total: AtomicU64,
    pub(super) queue_wait_max_ns: AtomicU64,
    pub(super) dispatch_wait_max_ns: AtomicU64,
    pub(super) eval_service_max_ns: AtomicU64,
    /// 多读任务共享报告时钟，避免把实际超过 2 秒的窗口当成固定 2 秒。
    pub(super) last_report: Mutex<Option<std::time::Instant>>,
    /// 上次报告时各源缓冲 storage_overflow 总和 (增量输出)
    pub(super) last_overflow_reported: AtomicU64,
}

impl DataPlaneMetrics {
    /// 评估队列溢出时累计丢弃帧数 (eval worker 调用)
    pub fn add_eval_dropped(&self, frames: u64) {
        self.eval_dropped.fetch_add(frames, Ordering::Relaxed);
        self.eval_dropped_total.fetch_add(frames, Ordering::Relaxed);
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )] // 诊断日志近似换算 (MB/s / ms), 数值精度不影响行为
    pub(super) fn report(&self) {
        let mut last_report = self.last_report.lock();
        let now = std::time::Instant::now();
        let elapsed = last_report.map_or(METRICS_REPORT_INTERVAL, |last| now.duration_since(last));
        if elapsed < METRICS_REPORT_INTERVAL {
            return;
        }
        *last_report = Some(now);
        let rx_msgs = self.rx_msgs.swap(0, Ordering::Relaxed);
        let lagged = self.lagged.swap(0, Ordering::Relaxed);
        let eval_dropped = self.eval_dropped.swap(0, Ordering::Relaxed);
        if rx_msgs == 0 && lagged == 0 && eval_dropped == 0 {
            return;
        }
        let secs = elapsed.as_secs_f64();
        let batches = self.feed_batches.swap(0, Ordering::Relaxed).max(1);
        let feed_ns = self.feed_ns.swap(0, Ordering::Relaxed);
        let eval_ns = self.eval_ns.swap(0, Ordering::Relaxed);
        let eval_batches = self.eval_batches.swap(0, Ordering::Relaxed);
        let frames_ingested = self.frames_ingested.swap(0, Ordering::Relaxed);
        let frames = self.frames_evaled.swap(0, Ordering::Relaxed);
        // 异步的两个平面不能相减，也不能共用批数作分母。产帧率只取实际
        // 摄入帧数，避免 fan-out 的已求值/丢弃计数把同一批数据成倍重复统计。
        let produced_per_sec = frames_ingested as f64 / secs;
        let eval_avg_ms = eval_ns as f64 / eval_batches.max(1) as f64 / 1e6;
        let msg = format!(
            "数据平面指标: rx {:.1}MB/s ({} 消息/s) | ingest {} 批, 均 {:.2}ms, \
             帧均 {}/批, 产帧≈{:.0}/s | eval {} 批, 均 {:.2}ms, 消费 {} 帧 \
             | Lagged 丢弃 {} 条, 评估队列丢弃 {} 帧",
            self.rx_bytes.swap(0, Ordering::Relaxed) as f64 / secs / 1e6,
            (rx_msgs as f64 / secs) as u64,
            batches,
            feed_ns as f64 / batches as f64 / 1e6,
            frames_ingested / batches,
            produced_per_sec,
            eval_batches,
            eval_avg_ms,
            frames,
            lagged,
            eval_dropped,
        );
        if lagged > 0 || eval_dropped > 0 {
            log::warn!("{msg}");
        } else {
            log::debug!("{msg}");
        }
    }
}
