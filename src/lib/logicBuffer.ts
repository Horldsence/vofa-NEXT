import type { LogicSample, DecodedEvent } from '../types';

/// 逻辑采样环形缓冲区 — 接收来自后端 transport:logic-samples 事件和 subscribe_logic_samples Channel
/// 支持订阅通知 + 最近 N 条查询
class LogicSampleBuffer {
  private samples: LogicSample[] = [];
  private capacity: number;
  private listeners = new Set<(samples: LogicSample[]) => void>();
  private _version = 0;

  constructor(capacity = 20000) {
    this.capacity = capacity;
  }

  /// 批量推入采样
  push(batch: LogicSample[]) {
    if (batch.length === 0) return;
    this.samples.push(...batch);
    // 超容量时丢弃旧采样
    if (this.samples.length > this.capacity) {
      this.samples.splice(0, this.samples.length - this.capacity);
    }
    this._version++;
    this.notify();
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
    this.notify();
  }

  get length(): number {
    return this.samples.length;
  }

  get version(): number {
    return this._version;
  }

  subscribe(fn: (samples: LogicSample[]) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private notify() {
    const recent = this.getRecent(1000);
    this.listeners.forEach((fn) => fn(recent));
  }
}

/// 解码事件环形缓冲区
class DecodedEventBuffer {
  private events: DecodedEvent[] = [];
  private capacity: number;
  private listeners = new Set<(events: DecodedEvent[]) => void>();
  private _version = 0;

  constructor(capacity = 10000) {
    this.capacity = capacity;
  }

  /// 批量推入事件
  push(batch: DecodedEvent[]) {
    if (batch.length === 0) return;
    this.events.push(...batch);
    // 超容量时丢弃旧事件
    if (this.events.length > this.capacity) {
      this.events.splice(0, this.events.length - this.capacity);
    }
    this._version++;
    this.notify();
  }

  /// 获取最近 N 条事件 (返回顺序: 旧→新)
  getRecent(count: number): DecodedEvent[] {
    const n = Math.min(count, this.events.length);
    return this.events.slice(this.events.length - n);
  }

  clear() {
    this.events = [];
    this._version++;
    this.notify();
  }

  get length(): number {
    return this.events.length;
  }

  get version(): number {
    return this._version;
  }

  subscribe(fn: (events: DecodedEvent[]) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private notify() {
    const recent = this.getRecent(500);
    this.listeners.forEach((fn) => fn(recent));
  }
}

/// 全局逻辑采样缓冲区
export const logicSampleBuffer = new LogicSampleBuffer();
/// 全局解码事件缓冲区
export const decodedEventBuffer = new DecodedEventBuffer();
