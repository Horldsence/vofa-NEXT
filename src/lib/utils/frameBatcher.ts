/// 帧级批处理器 — 把同一帧内的多次推送合并为一次回调
/// 高频数据路径 (buffer 通知 / store 更新) 中, push 只保留最新值,
/// 每帧最多触发一次 onFlush(最新值), 避免每批次都触发一次 React 渲染。
/// raf/caf 可注入, 测试用手工 ticker 替代真实 rAF 以保持行为确定。
export interface FrameBatcher<T> {
  push: (value: T) => void;
  cancel: () => void;
}

export function createFrameBatcher<T>(
  onFlush: (value: T) => void,
  raf: (cb: FrameRequestCallback) => number = (cb) => requestAnimationFrame(cb),
  caf: (id: number) => void = (id) => cancelAnimationFrame(id)
): FrameBatcher<T> {
  let pending: T | null = null;
  let hasPending = false;
  let rafId: number | null = null;

  return {
    push(value) {
      pending = value;
      hasPending = true;
      if (rafId !== null) return;
      rafId = raf(() => {
        rafId = null;
        if (!hasPending) return;
        hasPending = false;
        const latest = pending as T;
        pending = null;
        onFlush(latest);
      });
    },
    cancel() {
      if (rafId !== null) {
        caf(rafId);
        rafId = null;
      }
      hasPending = false;
      pending = null;
    },
  };
}
