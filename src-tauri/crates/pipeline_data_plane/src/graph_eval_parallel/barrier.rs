//! 并发原语 — 自旋屏障 + staging 交换槽

use std::sync::atomic::{AtomicUsize, Ordering};

use super::plan::WorkerState;

/// 自旋屏障 — 分块工作在微秒~百微秒量级, `std::sync::Barrier` 的
/// 互斥锁/条件变量睡眠唤醒 (~50µs) 会吃掉并行收益; 这里先自旋后让出
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

    pub(super) fn wait(&self) {
        let arrived = self.count.fetch_add(1, Ordering::AcqRel) + 1;
        if arrived == self.threads {
            // 最后到达者: 开启新代次放行全体
            self.count.store(0, Ordering::Release);
            self.generation.fetch_add(1, Ordering::Release);
            return;
        }
        let gen = self.generation.load(Ordering::Acquire);
        let mut spins = 0usize;
        while self.generation.load(Ordering::Acquire) == gen {
            if spins < 8_192 {
                std::hint::spin_loop();
            } else {
                std::thread::yield_now();
            }
            spins = spins.wrapping_add(1);
        }
    }
}

/// staging 交换槽 — worker 与协调者经互斥锁成对交接
pub(super) struct StageSlot {
    pub(super) staged_derived: Vec<(usize, f32)>,
    pub(super) staged_spectra: Vec<(u32, f32)>,
    pub(super) snapshot_delta: Option<(node_engine::ValuesMap, node_engine::StringValuesMap)>,
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
