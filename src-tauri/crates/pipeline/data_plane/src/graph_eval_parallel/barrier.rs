//! 并发原语 — 自旋屏障 + staging 交换槽

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::plan::WorkerState;

/// 自旋屏障：短批优先减少休眠唤醒开销，长等待通过 yield 让出执行权。
pub(super) struct SpinBarrier {
    threads: usize,
    count: AtomicUsize,
    generation: AtomicUsize,
}

impl SpinBarrier {
    pub(super) const fn new(threads: usize) -> Self {
        Self {
            threads,
            count: AtomicUsize::new(0),
            generation: AtomicUsize::new(0),
        }
    }

    pub(super) fn wait(&self, broken: &AtomicBool) {
        // 必须先读取世代再报到。报到后才读取会漏掉最后一个线程的放行，
        // 把新世代当成等待目标，所有参与者永远卡在不同屏障。
        let generation = self.generation.load(Ordering::Acquire);
        let arrived = self.count.fetch_add(1, Ordering::AcqRel) + 1;
        if arrived == self.threads {
            // 最后到达者: 开启新代次放行全体
            self.count.store(0, Ordering::Release);
            self.generation.fetch_add(1, Ordering::Release);
            return;
        }
        let mut spins = 0usize;
        while self.generation.load(Ordering::Acquire) == generation {
            if broken.load(Ordering::Acquire) {
                return;
            }
            if spins < 8_192 {
                std::hint::spin_loop();
            } else {
                std::thread::yield_now();
            }
            spins = spins.wrapping_add(1);
        }
    }
}

/// 任一参与者在计算、物化或回放时 panic 都必须释放其他等待者。
pub(super) struct PanicSignal<'a>(pub(super) &'a AtomicBool);

impl Drop for PanicSignal<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.store(true, Ordering::Release);
        }
    }
}

/// staging 交换槽 — worker 与协调者经互斥锁成对交接
pub(super) struct StageSlot {
    pub(super) staged_derived: Vec<(usize, u64, f32)>,
    pub(super) staged_spectra: Vec<(u32, f32)>,
    pub(super) snapshot_delta: Option<(engine::ValuesMap, engine::StringValuesMap)>,
}

impl StageSlot {
    pub(super) const fn new() -> Self {
        Self {
            staged_derived: Vec::new(),
            staged_spectra: Vec::new(),
            snapshot_delta: None,
        }
    }

    /// 与 worker 私有 staging 原位交换 (worker 拿回旧缓冲复用, 协调者取得本块产物)
    pub(super) const fn swap_from(&mut self, ws: &mut WorkerState) {
        std::mem::swap(&mut self.staged_derived, &mut ws.staged_derived);
        std::mem::swap(&mut self.staged_spectra, &mut ws.staged_spectra);
        std::mem::swap(&mut self.snapshot_delta, &mut ws.snapshot_delta);
    }
}

#[cfg(test)]
mod tests {
    use super::{PanicSignal, SpinBarrier};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn repeated_generations_do_not_lose_release() {
        let barrier = Arc::new(SpinBarrier::new(4));
        let broken = Arc::new(AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut workers = Vec::new();
        for _ in 0..4 {
            let barrier = barrier.clone();
            let broken = broken.clone();
            let tx = tx.clone();
            workers.push(std::thread::spawn(move || {
                for _ in 0..20_000 {
                    barrier.wait(&broken);
                    if broken.load(Ordering::Acquire) {
                        return;
                    }
                }
                tx.send(()).unwrap();
            }));
        }
        let finished = (0..4).all(|_| rx.recv_timeout(Duration::from_secs(10)).is_ok());
        broken.store(true, Ordering::Release);
        for worker in workers {
            worker.join().unwrap();
        }
        assert!(finished, "屏障不得遗漏代次放行");
    }

    #[test]
    fn panic_releases_waiter_without_all_participants() {
        let barrier = Arc::new(SpinBarrier::new(2));
        let broken = Arc::new(AtomicBool::new(false));
        let barrier_worker = barrier;
        let broken_worker = broken.clone();
        let waiter = std::thread::spawn(move || barrier_worker.wait(&broken_worker));
        let result = std::panic::catch_unwind(|| {
            let _signal = PanicSignal(&broken);
            panic!("模拟协调者提前退出");
        });
        assert!(result.is_err());
        waiter.join().unwrap();
    }
}
