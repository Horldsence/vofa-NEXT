import type { CanFrame } from '../types';

/// CAN 帧环形缓冲区 — 接收来自后端 transport:can-frames 事件和 subscribe_can_frames Channel
/// 支持订阅通知 + 最近 N 帧查询 + 过滤
class CanFrameBuffer {
  private frames: CanFrame[] = [];
  private capacity: number;
  private listeners = new Set<(frames: CanFrame[]) => void>();
  private _version = 0;

  constructor(capacity = 5000) {
    this.capacity = capacity;
  }

  /// 批量推入帧
  push(batch: CanFrame[]) {
    if (batch.length === 0) return;
    this.frames.push(...batch);
    // 超容量时丢弃旧帧
    if (this.frames.length > this.capacity) {
      this.frames.splice(0, this.frames.length - this.capacity);
    }
    this._version++;
    this.notify();
  }

  /// 获取最近 N 帧 (返回顺序: 旧→新)
  getRecent(count: number): CanFrame[] {
    const n = Math.min(count, this.frames.length);
    return this.frames.slice(this.frames.length - n);
  }

  /// 获取全部帧 (受容量限制)
  getAll(): CanFrame[] {
    return this.frames.slice();
  }

  /// 按 ID 过滤
  getById(id: number, extended: boolean): CanFrame[] {
    return this.frames.filter((f) => f.id === id && f.extended === extended);
  }

  clear() {
    this.frames = [];
    this._version++;
    this.notify();
  }

  get length(): number {
    return this.frames.length;
  }

  get version(): number {
    return this._version;
  }

  subscribe(fn: (frames: CanFrame[]) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private notify() {
    const recent = this.getRecent(200);
    this.listeners.forEach((fn) => fn(recent));
  }
}

/// 全局 CAN 帧缓冲区
export const canFrameBuffer = new CanFrameBuffer();
