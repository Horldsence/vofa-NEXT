import type { LogicSample, DecodedEvent } from '../types';

/// 逻辑采样环形缓冲区 — 接收来自后端 subscribe_logic_samples Channel
/// RAF 节流: 同一帧内多次 push 只通知一次, 避免高频批次导致 React 过度渲染
/// 引用稳定: 数据未变化时不创建新数组, 避免 zustand 浅比较失效
class LogicSampleBuffer {
  private samples: LogicSample[] = [];
  private _capacity: number;
  private listeners = new Set<(samples: LogicSample[]) => void>();
  private _version = 0;
  private lastSnapshot: LogicSample[] = [];
  private lastSnapshotCount = -1;
  private rafScheduled = false;
  private statsListeners = new Set<(usage: number, length: number, capacity: number) => void>();

  constructor(capacity = 20000) {
    this._capacity = capacity;
  }

  /// 批量推入采样
  push(batch: LogicSample[]) {
    if (batch.length === 0) return;
    this.samples.push(...batch);
    // 超容量时丢弃旧采样
    if (this.samples.length > this._capacity) {
      this.samples.splice(0, this.samples.length - this._capacity);
    }
    this._version++;
    this.scheduleNotify();
  }

  /// 获取最近 N 条采样 (返回顺序: 旧→新)
  getRecent(count: number): LogicSample[] {
    const n = Math.min(count, this.samples.length);
    return this.samples.slice(this.samples.length - n);
  }

  /// 获取全部采样 (受容量限制)
  getAll(): LogicSample[] {
    return this.samples.slice();
  }

  clear() {
    this.samples = [];
    this._version++;
    this.lastSnapshot = [];
    this.lastSnapshotCount = -1;
    this.scheduleNotify();
  }

  get length(): number {
    return this.samples.length;
  }

  get capacity(): number {
    return this._capacity;
  }

  get version(): number {
    return this._version;
  }

  /// 订阅采样更新 (RAF 节流后触发)
  subscribe(fn: (samples: LogicSample[]) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /// 订阅缓存使用量统计 (RAF 节流后触发), usage ∈ [0,1]
  subscribeStats(fn: (usage: number, length: number, capacity: number) => void): () => void {
    this.statsListeners.add(fn);
    fn(this.samples.length / this._capacity, this.samples.length, this._capacity);
    return () => this.statsListeners.delete(fn);
  }

  private scheduleNotify() {
    if (this.rafScheduled) return;
    this.rafScheduled = true;
    requestAnimationFrame(() => {
      this.rafScheduled = false;
      this.flushNotify();
    });
  }

  private flushNotify() {
    const count = this.samples.length;
    if (count !== this.lastSnapshotCount) {
      this.lastSnapshot = this.getRecent(1000);
      this.lastSnapshotCount = count;
    }
    const snapshot = this.lastSnapshot;
    this.listeners.forEach((fn) => fn(snapshot));

    const usage = count / this._capacity;
    this.statsListeners.forEach((fn) => fn(usage, count, this._capacity));
  }
}

/// 解码事件环形缓冲区
class DecodedEventBuffer {
  private events: DecodedEvent[] = [];
  private _capacity: number;
  private listeners = new Set<(events: DecodedEvent[]) => void>();
  private _version = 0;
  private lastSnapshot: DecodedEvent[] = [];
  private lastSnapshotCount = -1;
  private rafScheduled = false;
  private statsListeners = new Set<(usage: number, length: number, capacity: number) => void>();

  constructor(capacity = 10000) {
    this._capacity = capacity;
  }

  /// 批量推入事件
  push(batch: DecodedEvent[]) {
    if (batch.length === 0) return;
    this.events.push(...batch);
    // 超容量时丢弃旧事件
    if (this.events.length > this._capacity) {
      this.events.splice(0, this.events.length - this._capacity);
    }
    this._version++;
    this.scheduleNotify();
  }

  /// 获取最近 N 条事件 (返回顺序: 旧→新)
  getRecent(count: number): DecodedEvent[] {
    const n = Math.min(count, this.events.length);
    return this.events.slice(this.events.length - n);
  }

  clear() {
    this.events = [];
    this._version++;
    this.lastSnapshot = [];
    this.lastSnapshotCount = -1;
    this.scheduleNotify();
  }

  get length(): number {
    return this.events.length;
  }

  get capacity(): number {
    return this._capacity;
  }

  get version(): number {
    return this._version;
  }

  /// 订阅事件更新 (RAF 节流后触发)
  subscribe(fn: (events: DecodedEvent[]) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /// 订阅缓存使用量统计 (RAF 节流后触发), usage ∈ [0,1]
  subscribeStats(fn: (usage: number, length: number, capacity: number) => void): () => void {
    this.statsListeners.add(fn);
    fn(this.events.length / this._capacity, this.events.length, this._capacity);
    return () => this.statsListeners.delete(fn);
  }

  private scheduleNotify() {
    if (this.rafScheduled) return;
    this.rafScheduled = true;
    requestAnimationFrame(() => {
      this.rafScheduled = false;
      this.flushNotify();
    });
  }

  private flushNotify() {
    const count = this.events.length;
    if (count !== this.lastSnapshotCount) {
      this.lastSnapshot = this.getRecent(500);
      this.lastSnapshotCount = count;
    }
    const snapshot = this.lastSnapshot;
    this.listeners.forEach((fn) => fn(snapshot));

    const usage = count / this._capacity;
    this.statsListeners.forEach((fn) => fn(usage, count, this._capacity));
  }
}

/// 全局逻辑采样缓冲区
export const logicSampleBuffer = new LogicSampleBuffer();
/// 全局解码事件缓冲区
export const decodedEventBuffer = new DecodedEventBuffer();
