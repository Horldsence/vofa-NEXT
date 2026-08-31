import { describe, expect, it, vi } from 'vitest';
import { createFrameBatcher } from '../utils/frameBatcher';

/// 手工 rAF ticker — 完全确定, 不依赖 jsdom 计时
function manualTicker() {
  let queued: { id: number; cb: FrameRequestCallback }[] = [];
  let nextId = 1;
  const raf = (cb: FrameRequestCallback): number => {
    const id = nextId++;
    queued.push({ id, cb });
    return id;
  };
  const caf = (id: number): void => {
    queued = queued.filter((q) => q.id !== id);
  };
  const flush = (): void => {
    const items = queued.splice(0);
    for (const { cb } of items) cb(0);
  };
  const flushAll = (): void => {
    while (queued.length > 0) flush();
  };
  return { raf, caf, flush, flushAll, pending: () => queued.length };
}

describe('createFrameBatcher', () => {
  it('coalesces a burst of pushes within one frame into a single flush of the latest value', () => {
    const t = manualTicker();
    const onFlush = vi.fn();
    const batcher = createFrameBatcher<number>(onFlush, t.raf, t.caf);

    for (let i = 0; i < 100; i++) batcher.push(i);

    expect(onFlush).not.toHaveBeenCalled();
    expect(t.pending()).toBe(1);
    t.flush();
    expect(onFlush).toHaveBeenCalledTimes(1);
    expect(onFlush).toHaveBeenCalledWith(99);
  });

  it('flushes at most once per frame across multiple frames', () => {
    const t = manualTicker();
    const onFlush = vi.fn();
    const batcher = createFrameBatcher<number>(onFlush, t.raf, t.caf);

    batcher.push(1);
    t.flush();
    batcher.push(2);
    batcher.push(3);
    t.flush();
    batcher.push(4);
    t.flush();

    expect((onFlush.mock.calls as [number][]).map((c) => c[0])).toEqual([1, 3, 4]);
  });

  it('drops the pending value after cancel', () => {
    const t = manualTicker();
    const onFlush = vi.fn();
    const batcher = createFrameBatcher<number>(onFlush, t.raf, t.caf);

    batcher.push(1);
    batcher.cancel();
    t.flush();
    expect(onFlush).not.toHaveBeenCalled();

    batcher.push(2);
    t.flush();
    expect(onFlush).toHaveBeenCalledWith(2);
  });
});
