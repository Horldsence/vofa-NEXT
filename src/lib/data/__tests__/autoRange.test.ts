import { describe, expect, it, vi } from 'vitest';
import { getAutoRangeStore, releaseAutoRangeStore } from '../autoRange';

const sample = (seq: number, ts: number, value: number) => ({ seq, ts, value });

describe('autoRange store', () => {
  it('notifies only when the snapped range actually changes', () => {
    vi.useFakeTimers();
    const store = getAutoRangeStore('test-change');
    let notifications = 0;
    const unsubscribe = store.subscribe(() => notifications++);

    store.push([sample(1, 0, 2), sample(2, 10, 46)]);
    // span 44 → 4 格 raw 11 → step 20 → 边界 [0, 60]
    expect(store.getSnapshot().range).toEqual({ min: 0, max: 60 });
    expect(notifications).toBe(1);

    // 窗口内的微小波动不改变 1-2-5 档位 → 不通知
    store.push([sample(3, 260, 8), sample(4, 270, 44)]);
    expect(notifications).toBe(1);
    unsubscribe();
    releaseAutoRangeStore('test-change');
  });

  it('prunes samples older than the window so the range can shrink', () => {
    vi.useFakeTimers();
    const store = getAutoRangeStore('test-window');
    store.setWindow(1); // 1s 窗口
    store.push([sample(1, 0, 100)]);
    store.push([sample(2, 1500, 1), sample(3, 1600, 2)]);
    // 极值 100 (ts=0) 已滚出 1s 窗口, 量程收缩到 [1, 2]
    expect(store.getSnapshot().range).toEqual({ min: 1, max: 2 });
    expect(store.sampleCount()).toBe(2);
    releaseAutoRangeStore('test-window');
  });

  it('falls back to ±1 range around rounded center for flat signals', () => {
    vi.useFakeTimers();
    const store = getAutoRangeStore('test-flat');
    store.push([sample(1, 0, 3.3), sample(2, 10, 3.3)]);
    expect(store.getSnapshot().range).toEqual({ min: 2, max: 4 });
    releaseAutoRangeStore('test-flat');
  });

  it('caps the buffer at MAX_POINTS by dropping oldest', () => {
    vi.useFakeTimers();
    const store = getAutoRangeStore('test-cap');
    const batch = Array.from({ length: 25_000 }, (_, i) => sample(i, i, i));
    store.push(batch);
    expect(store.sampleCount()).toBeLessThanOrEqual(20_000);
    releaseAutoRangeStore('test-cap');
  });

  it('dispose stops notifications and release yields a fresh store', () => {
    vi.useFakeTimers();
    const store = getAutoRangeStore('test-dispose');
    let notifications = 0;
    store.subscribe(() => notifications++);
    store.push([sample(1, 0, 5)]);
    expect(notifications).toBe(1);
    releaseAutoRangeStore('test-dispose');
    // release 后重新获取是新实例
    const fresh = getAutoRangeStore('test-dispose');
    expect(fresh.sampleCount()).toBe(0);
    releaseAutoRangeStore('test-dispose');
  });

  it('throttles recomputation when data clock advances slowly', () => {
    vi.useFakeTimers();
    const store = getAutoRangeStore('test-throttle');
    // 50 次推送只推进 500ms 数据时钟 — 重算被节流到极少数次
    let notifications = 0;
    store.subscribe(() => notifications++);
    for (let i = 0; i < 50; i++) {
      store.push([sample(i, i * 10, i)]);
    }
    vi.runAllTimers();
    expect(notifications).toBeLessThanOrEqual(5);
    releaseAutoRangeStore('test-throttle');
  });
});
