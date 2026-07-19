import type { CanFrame } from '../types';

/// CAN 帧环形缓冲区 — 接收来自后端 subscribe_can_frames Channel
/// RAF 节流: 同一帧内多次 push 只通知一次, 避免高频批次导致 React 过度渲染
/// 引用稳定: 数据未变化时不创建新数组, 避免 zustand 浅比较失效
class CanFrameBuffer {
  private frames: CanFrame[] = [];
  private _capacity: number;
  private listeners = new Set<(frames: CanFrame[]) => void>();
  private _version = 0;
  /// 最近一次 notify 发出的引用 (用于引用稳定化)
  private lastSnapshot: CanFrame[] = [];
  private lastSnapshotCount = -1;
  /// RAF 节流标志
  private rafScheduled = false;
  /// 状态栏订阅: 缓存使用量 (0-1) + 长度
  private statsListeners = new Set<(usage: number, length: number, capacity: number) => void>();

  constructor(capacity = 100_000) {
    this._capacity = capacity;
  }

  /// 批量推入帧
  push(batch: CanFrame[]) {
    if (batch.length === 0) return;
    this.frames.push(...batch);
    // 超容量时丢弃旧帧
    if (this.frames.length > this._capacity) {
      this.frames.splice(0, this.frames.length - this._capacity);
    }
    this._version++;
    this.scheduleNotify();
  }

  /// 按索引获取一帧 (0 = 最旧, length-1 = 最新)
  getFrame(index: number): CanFrame | undefined {
    if (index < 0 || index >= this.frames.length) return undefined;
    return this.frames[index];
  }

  /// 获取最近 N 帧 (返回顺序: 旧→新)
  /// 注意: 每次调用都返回新数组, 组件应缓存结果或通过 subscribe 订阅
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
    this.lastSnapshot = [];
    this.lastSnapshotCount = -1;
    this.scheduleNotify();
  }

  get length(): number {
    return this.frames.length;
  }

  get capacity(): number {
    return this._capacity;
  }

  /// 设置容量并裁剪超额数据
  setCapacity(capacity: number) {
    this._capacity = Math.max(1, capacity);
    if (this.frames.length > this._capacity) {
      this.frames.splice(0, this.frames.length - this._capacity);
      this._version++;
      this.lastSnapshot = [];
      this.lastSnapshotCount = -1;
      this.scheduleNotify();
    }
    // 容量变化时立即刷新 stats
    this.flushNotify();
  }

  get version(): number {
    return this._version;
  }

  /// 订阅帧数据更新 (RAF 节流后触发)
  subscribe(fn: (frames: CanFrame[]) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /// 订阅缓存使用量统计 (RAF 节流后触发), usage ∈ [0,1]
  subscribeStats(fn: (usage: number, length: number, capacity: number) => void): () => void {
    this.statsListeners.add(fn);
    // 立即推送一次当前状态
    fn(this.frames.length / this._capacity, this.frames.length, this._capacity);
    return () => this.statsListeners.delete(fn);
  }

  /// RAF 节流: 同一帧内多次 push 合并为一次通知
  private scheduleNotify() {
    if (this.rafScheduled) return;
    this.rafScheduled = true;
    // requestAnimationFrame 在浏览器环境可用; 在 Tauri webview 中同样可用
    requestAnimationFrame(() => {
      this.rafScheduled = false;
      this.flushNotify();
    });
  }

  private flushNotify() {
    const count = this.frames.length;
    // 引用稳定化: 如果长度未变, 复用上一次快照, 避免 React 浅比较失效
    if (count !== this.lastSnapshotCount) {
      this.lastSnapshot = this.frames.slice();
      this.lastSnapshotCount = count;
    }
    const snapshot = this.lastSnapshot;
    this.listeners.forEach((fn) => fn(snapshot));

    // 通知统计订阅者
    const usage = count / this._capacity;
    this.statsListeners.forEach((fn) => fn(usage, count, this._capacity));
  }
}

/// 全局 CAN 帧缓冲区
export const canFrameBuffer = new CanFrameBuffer();
