// ============ 显示控件自动量程 ============
//
// 滑动窗口自适应量程 (与示波器 AutoSet 同思路): 只统计最近 windowSec 秒的
// 样本, 窗口滚出旧数据后量程可收缩, 避免全历史极值把显示压扁。
//
// 对 React 以 external store 暴露 (useSyncExternalStore 消费): 快照是经
// 1-2-5 取整的量程, 只在档位真正变化时通知 — 高频样本推送不会引发额外
// 重渲染; 重算本身节流 ≤5Hz。store 按控件 id 注册, 卸载时 release 释放。

import { computeNiceRange, type DisplayRange } from '../utils/valueRange';

/// 样本缓冲硬上限 — 超限丢最旧 (窗口裁剪正常情况下先于触顶生效;
/// 极高采样率下等价于「量程跟随最近一段突发」, 可接受的降级)
const MAX_POINTS = 20_000;
/// 重算节流间隔 (数据时钟 ms)
const RECOMPUTE_INTERVAL_MS = 200;

export interface AutoRangeSnapshot {
  readonly range: DisplayRange;
}

export interface AutoRangeSample {
  readonly seq: number;
  readonly ts: number;
  readonly value: number;
}

/// 方法一律用属性签名 (实现为箭头/绑定方法) — 消费方解构或传引用
/// (useSyncExternalStore) 时不触发 unbound-method 语义
export interface AutoRangeStore {
  push: (samples: readonly AutoRangeSample[]) => void;
  setWindow: (windowSec: number) => void;
  setTicks: (majorTicks: number) => void;
  subscribe: (listener: () => void) => () => void;
  getSnapshot: () => AutoRangeSnapshot;
  sampleCount: () => number;
}

const stores = new Map<string, AutoRangeStoreImpl>();

/// 取 (或创建) 控件对应的自动量程 store — key 用控件 id (每控件单数值输入)
export function getAutoRangeStore(key: string): AutoRangeStore {
  let store = stores.get(key);
  if (!store) {
    store = new AutoRangeStoreImpl();
    stores.set(key, store);
  }
  return store;
}

/// 控件卸载时释放, 防止缓冲与监听器泄漏
export function releaseAutoRangeStore(key: string): void {
  stores.get(key)?.dispose();
  stores.delete(key);
}

class AutoRangeStoreImpl implements AutoRangeStore {
  private points: AutoRangeSample[] = [];
  private windowMs = 10_000;
  private ticks = 5;
  private listeners = new Set<() => void>();
  private snapshot: AutoRangeSnapshot = { range: { min: 0, max: 1 } };
  private lastComputeTs = -Infinity;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private disposed = false;

  push(samples: readonly AutoRangeSample[]): void {
    if (this.disposed || samples.length === 0) return;
    for (const sample of samples) {
      if (Number.isFinite(sample.ts) && Number.isFinite(sample.value)) {
        this.points.push(sample);
      }
    }
    const lastTs = this.points[this.points.length - 1].ts;
    // 窗口裁剪: 丢掉 windowSec 之前的样本 (ts 升序)
    const cutoff = lastTs - this.windowMs;
    let drop = 0;
    while (drop < this.points.length && this.points[drop].ts < cutoff) drop++;
    if (drop > 0) this.points.splice(0, drop);
    if (this.points.length > MAX_POINTS) {
      this.points.splice(0, this.points.length - MAX_POINTS);
    }
    this.scheduleCompute(lastTs);
  }

  setWindow(windowSec: number): void {
    const next = Number.isFinite(windowSec) && windowSec > 0 ? windowSec * 1000 : 10_000;
    if (next === this.windowMs) return;
    this.windowMs = next;
    if (this.points.length > 0) this.scheduleCompute(this.points[this.points.length - 1].ts);
  }

  setTicks(majorTicks: number): void {
    const next = Math.max(2, Math.min(11, Math.round(majorTicks) || 5));
    if (next === this.ticks) return;
    this.ticks = next;
    if (this.points.length > 0) this.scheduleCompute(this.points[this.points.length - 1].ts);
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): AutoRangeSnapshot => this.snapshot;

  sampleCount = (): number => this.points.length;

  dispose(): void {
    this.disposed = true;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    this.listeners.clear();
    this.points = [];
  }

  /// 节流重算 — 数据时钟推进不足间隔时安排一次尾随计算
  private scheduleCompute(nowTs: number): void {
    if (this.disposed) return;
    const elapsed = nowTs - this.lastComputeTs;
    if (elapsed >= RECOMPUTE_INTERVAL_MS) {
      this.computeNow();
      this.lastComputeTs = nowTs;
      return;
    }
    if (this.timer !== null) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      if (this.disposed || this.points.length === 0) return;
      this.computeNow();
      this.lastComputeTs = this.points[this.points.length - 1].ts;
    }, RECOMPUTE_INTERVAL_MS - elapsed);
  }

  private computeNow(): void {
    let vMin = Infinity;
    let vMax = -Infinity;
    for (const point of this.points) {
      const v = point.value;
      if (v < vMin) vMin = v;
      if (v > vMax) vMax = v;
    }
    if (vMin === Infinity) return;
    const next = computeNiceRange(vMin, vMax, this.ticks);
    if (next.min === this.snapshot.range.min && next.max === this.snapshot.range.max) return;
    this.snapshot = { range: next };
    for (const listener of this.listeners) listener();
  }
}
