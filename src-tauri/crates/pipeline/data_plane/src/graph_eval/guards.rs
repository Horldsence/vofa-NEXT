//! `mem::take` 守卫 — 并发路径整表取出的 panic 安全写回

/// `mem::take` + Drop 写回守卫 — 并发路径把共享表整表取出做零克隆共享,
/// 任何退路 (含 panic 展开) 都把当前值原样放回, 不丢共享状态
pub struct PutBack<'a, T> {
    dst: &'a mut T,
    val: Option<T>,
}

impl<'a, T: Default> PutBack<'a, T> {
    pub fn take(dst: &'a mut T) -> Self {
        Self {
            val: Some(std::mem::take(dst)),
            dst,
        }
    }

    pub const fn get(&self) -> &T {
        self.val.as_ref().expect("PutBack 值仅在 Drop 时移出")
    }

    pub const fn get_mut(&mut self) -> &mut T {
        self.val.as_mut().expect("PutBack 值仅在 Drop 时移出")
    }
}

impl<T> Drop for PutBack<'_, T> {
    fn drop(&mut self) {
        if let Some(v) = self.val.take() {
            *self.dst = v;
        }
    }
}

/// 持锁版整表取出 — 锁 guard 与取出值一起持有, Drop 时原样写回
/// (调用方无需再绑定 mut 锁 guard, 并发路径直接跨线程共享 `get()`)
pub struct TakeGuard<'a, T> {
    guard: parking_lot::MutexGuard<'a, T>,
    val: Option<T>,
}

impl<'a, T: Default> TakeGuard<'a, T> {
    pub fn take(mut guard: parking_lot::MutexGuard<'a, T>) -> Self {
        Self {
            val: Some(std::mem::take(&mut *guard)),
            guard,
        }
    }

    pub const fn get(&self) -> &T {
        self.val.as_ref().expect("TakeGuard 值仅在 Drop 时移出")
    }

    pub const fn get_mut(&mut self) -> &mut T {
        self.val.as_mut().expect("TakeGuard 值仅在 Drop 时移出")
    }
}

impl<T> Drop for TakeGuard<'_, T> {
    fn drop(&mut self) {
        if let Some(v) = self.val.take() {
            *self.guard = v;
        }
    }
}
