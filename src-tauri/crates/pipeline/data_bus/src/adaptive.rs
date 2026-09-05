//! 数据平面批处理/worker 自动调节器 — 迟滞避免负载边界反复扩缩

use std::time::Duration;

use crate::types::RuntimeLimits;

/// 数据平面批处理/worker 自动调节器。用迟滞避免负载边界反复扩缩。
#[derive(Debug, Clone)]
pub struct AdaptiveController {
    workers: usize,
    high_streak: u8,
    low_since: Option<std::time::Instant>,
    target_batch_bytes: usize,
    /// 吞吐 regime 的服务时间目标 (ms)。积压持续时 16→64 上探:
    /// 占空比 ≈ service_time/target, 目标必须大于服务时间才能排空积压。
    service_target_ms: u32,
    ewma_input_bytes_per_sec: u64,
    last_observed: std::time::Instant,
}

impl Default for AdaptiveController {
    fn default() -> Self {
        Self {
            workers: 1,
            high_streak: 0,
            low_since: None,
            target_batch_bytes: 64 * 1024,
            service_target_ms: 16,
            ewma_input_bytes_per_sec: 0,
            last_observed: std::time::Instant::now(),
        }
    }
}

impl AdaptiveController {
    pub fn observe(
        &mut self,
        queue_fill: f64,
        queue_age: Duration,
        service_time: Duration,
        input_bytes: usize,
        limits: RuntimeLimits,
    ) {
        self.observe_at(
            std::time::Instant::now(),
            queue_fill,
            queue_age,
            service_time,
            input_bytes,
            limits,
        );
    }

    /// 单一观察时刻同时用于速率、扩缩容与基准测试，避免测试回拨后再读时钟。
    fn observe_at(
        &mut self,
        now: std::time::Instant,
        queue_fill: f64,
        queue_age: Duration,
        service_time: Duration,
        input_bytes: usize,
        limits: RuntimeLimits,
    ) {
        let elapsed_us = u64::try_from(
            now.saturating_duration_since(self.last_observed)
                .as_micros(),
        )
        .unwrap_or(u64::MAX)
        .max(1);
        self.last_observed = now;
        let current_rate = u64::try_from(input_bytes)
            .unwrap_or(u64::MAX)
            .saturating_mul(1_000_000)
            / elapsed_us;
        self.ewma_input_bytes_per_sec = if self.ewma_input_bytes_per_sec == 0 {
            current_rate
        } else {
            self.ewma_input_bytes_per_sec
                .saturating_mul(4)
                .saturating_add(current_rate)
                / 5
        };

        let max_workers = limits
            .max_workers
            .min(std::thread::available_parallelism().map_or(1, std::num::NonZero::get))
            .max(1);
        if queue_fill > 0.5 || queue_age > Duration::from_millis(10) {
            self.high_streak = self.high_streak.saturating_add(1);
            self.low_since = None;
            if self.high_streak >= 3 && self.workers < max_workers {
                self.workers += 1;
                self.high_streak = 0;
            }
        } else if queue_fill < 0.1 && queue_age < Duration::from_millis(2) {
            let since = self.low_since.get_or_insert(now);
            if now.saturating_duration_since(*since) >= Duration::from_secs(2) && self.workers > 1 {
                self.workers -= 1;
                self.low_since = Some(now);
            }
        } else {
            self.high_streak = 0;
            self.low_since = None;
        }

        // —— 批大小策略: 吞吐/延迟双 regime ——
        // 旧策略「service_time > 16ms 就减半」在服务时间含数值平面评估 (数十毫秒)
        // 时会长期触发, 批被压到下限 → 同样输入速率下批次翻倍 → 占空比超 100% →
        // 持续 Lagged (越堵越切小批的死亡螺旋)。新策略:
        // 1. 到达不变量: 每批至少合流「上一批服务期间到达」的量
        //    (ewma_rate × service_time), 否则队列单调增长, 丢数据只是时间问题;
        // 2. 深队列 (吞吐 regime): 目标 = ewma_rate × service_target,
        //    service_target 随积压持续 16→64ms 上探, 摊薄每批固定开销;
        // 3. 浅队列 (延迟 regime): 目标 = ewma_rate × 8ms, 小批快转。
        let deep_queue = queue_fill > 0.5 || queue_age > Duration::from_millis(10);
        if deep_queue {
            self.service_target_ms = self.service_target_ms.saturating_mul(2).min(64);
        } else if queue_fill < 0.1 && queue_age < Duration::from_millis(2) {
            self.service_target_ms = (self.service_target_ms / 2).max(8);
        }
        let target_ms = u64::from(if deep_queue {
            self.service_target_ms
        } else {
            8
        });
        let rate_target = self.ewma_input_bytes_per_sec.saturating_mul(target_ms) / 1_000;
        // 到达下限同量纲换算: ewma[B/s] × service_ms / 1000 = 服务期内到达字节
        let service_floor = self
            .ewma_input_bytes_per_sec
            .saturating_mul(u64::try_from(service_time.as_millis()).unwrap_or(u64::MAX))
            / 1_000;
        let target = rate_target
            .max(service_floor)
            .clamp(16 * 1024_u64, 1024 * 1024_u64);
        let target = usize::try_from(target).unwrap_or(1024 * 1024);
        if target > self.target_batch_bytes {
            // 过载方向快速响应 (积压增长速度远快于平滑收敛)
            self.target_batch_bytes = target;
        } else {
            self.target_batch_bytes = (self.target_batch_bytes * 3 + target) / 4;
        }
    }

    #[must_use]
    pub const fn workers(&self) -> usize {
        self.workers
    }

    #[must_use]
    pub const fn target_batch_bytes(&self) -> usize {
        self.target_batch_bytes
    }

    #[must_use]
    pub const fn ewma_input_bytes_per_sec(&self) -> u64 {
        self.ewma_input_bytes_per_sec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造已建立 ewma 速率的控制器。回拨 last_observed 使 elapsed 恒为 dt,
    /// ewma 完全确定 (不依赖测试机的真实调度时序)。
    fn controller_with_ewma(rate_bps: u64) -> AdaptiveController {
        let mut controller = AdaptiveController::default();
        let input_per_observe = usize::try_from(rate_bps / 100).unwrap_or(usize::MAX);
        for _ in 0..8 {
            observe_dt(&mut controller, 10, 0.05, 1, 2, input_per_observe);
        }
        controller
    }

    /// 以确定的观察间隔驱动一次 observe
    fn observe_dt(
        controller: &mut AdaptiveController,
        dt_ms: u64,
        queue_fill: f64,
        queue_age_ms: u64,
        service_ms: u64,
        input_bytes: usize,
    ) {
        let now = std::time::Instant::now();
        controller.last_observed = now
            .checked_sub(Duration::from_millis(dt_ms))
            .expect("时钟早于当前时刻至少 dt, 系统时钟异常");
        controller.observe_at(
            now,
            queue_fill,
            Duration::from_millis(queue_age_ms),
            Duration::from_millis(service_ms),
            input_bytes,
            RuntimeLimits::default(),
        );
    }

    #[test]
    fn deep_queue_grows_batch_despite_slow_service() {
        // 旧策略回归: 服务时间 46ms (含数值平面评估) + 深积压时,
        // 批必须增长以摊薄固定开销, 而不是被「>16ms 减半」压到 16KB 下限
        // 不变量以控制器实际 ewma 为基准 (断言与测试机的调度时序无关):
        // 深队列下批次必须 ≥ 到达下限 (ewma × service_time), 否则队列单调增长。
        let mut controller = controller_with_ewma(512 * 1024);
        for _ in 0..12 {
            observe_dt(&mut controller, 10, 0.9, 40, 46, 512 * 1024 / 100);
        }
        let floor = controller.ewma_input_bytes_per_sec() * 46 / 1000;
        let floor = usize::try_from(floor.max(16 * 1024)).unwrap_or(usize::MAX);
        assert!(
            controller.target_batch_bytes() >= floor,
            "深队列下批 {} 低于到达下限 {floor}",
            controller.target_batch_bytes()
        );
    }

    #[test]
    fn slow_service_never_shrinks_batch_below_arrival_floor() {
        // 到达不变量: 持续 46ms 服务 + 512KB/s 输入时, 批不得小于服务期内到达量
        // (否则队列单调增长, 必然 Lagged)
        let mut controller = controller_with_ewma(512 * 1024);
        for _ in 0..12 {
            observe_dt(&mut controller, 10, 0.05, 1, 46, 512 * 1024 / 100);
        }
        // ewma 实际收敛值 = input_per_observe × 100 (5242 字节 / 10ms)
        let floor = (512 * 1024 / 100) * 100 * 46 / 1000;
        assert!(
            controller.target_batch_bytes() >= floor,
            "批 {} 低于到达下限 {floor}",
            controller.target_batch_bytes()
        );
    }

    #[test]
    fn shallow_queue_converges_to_latency_target() {
        // 浅队列回归延迟优先: 目标收敛到 ewma × 8ms 量级 (受 16KB 下限约束)
        let mut controller = controller_with_ewma(512 * 1024);
        // 先人为抬高到过载水位
        for _ in 0..4 {
            observe_dt(&mut controller, 10, 0.9, 40, 46, 512 * 1024 / 100);
        }
        for _ in 0..24 {
            observe_dt(&mut controller, 10, 0.05, 1, 2, 512 * 1024 / 100);
        }
        let latency_target = (512 * 1024 * 8 / 1000).clamp(16 * 1024, 1024 * 1024);
        assert!(
            controller.target_batch_bytes() <= latency_target * 2,
            "浅队列应收敛到延迟目标附近: {} (目标 {latency_target})",
            controller.target_batch_bytes()
        );
    }
}
