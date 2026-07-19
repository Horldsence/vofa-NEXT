import type { DataFrame, RawDataBatch, WaveformWindow } from '../types';

export const RAWDATA_BYTES_PER_ROW = 16;

/// 原始数据单行视图
export interface RawDataLineView {
  offset: number;
  timestamp: number;
  bytes: Uint8Array;
}

/// 原始数据快照 (兼容旧接口, 当前主要由虚拟列表直接按行读取)
export interface RawDataSnapshot {
  lines: RawDataLineView[];
  totalBytes: number;
}

/// 波形窗口缓存 — 接收来自后端 Tauri Channel 的推送, 由订阅者维护
/// 不同于旧的 WaveformBuffer (前端持有完整数据), 此处仅缓存最新窗口快照
class WaveformWindowCache {
  private latest: WaveformWindow = { timestamps: [], channels: [], channel_count: 0 };
  private _version = 0;
  private listeners = new Set<() => void>();
  private statsListeners = new Set<(usage: number, length: number, capacity: number) => void>();

  set(window: WaveformWindow) {
    this.latest = window;
    this._version++;
    this.notify();
    this.notifyStats();
  }

  get(): WaveformWindow {
    return this.latest;
  }

  get version(): number {
    return this._version;
  }

  clear() {
    this.latest = { timestamps: [], channels: [], channel_count: 0 };
    this._version++;
    this.notify();
    this.notifyStats();
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /// 订阅波形缓存使用率统计, usage ∈ [0,1]
  subscribeStats(fn: (usage: number, length: number, capacity: number) => void): () => void {
    this.statsListeners.add(fn);
    const capacity = Math.max(1, this.latest.buffer_capacity ?? 1);
    const length = this.latest.buffer_points ?? 0;
    fn(length / capacity, length, capacity);
    return () => this.statsListeners.delete(fn);
  }

  private notify() {
    this.listeners.forEach((fn) => fn());
  }

  private notifyStats() {
    const capacity = Math.max(1, this.latest.buffer_capacity ?? 1);
    const length = this.latest.buffer_points ?? 0;
    const usage = length / capacity;
    this.statsListeners.forEach((fn) => fn(usage, length, capacity));
  }
}

/// 全局波形窗口缓存
export const waveformWindow = new WaveformWindowCache();

/// 分片元数据 — 用于把字节偏移映射到时间戳
interface ChunkEntry {
  /// 该分片第一个字节在全局字节流中的偏移
  offset: number;
  /// 分片长度 (字节)
  length: number;
  /// 微秒时间戳
  timestamp_us: number;
}

/// 原始数据缓冲区 — 基于 Uint8Array 的环形缓冲区
/// 接收来自后端 subscribe_rawdata Channel 的 RawDataBatch, RAF 节流后通知订阅者
export class RawDataBuffer {
  private buf: Uint8Array;
  private writePos = 0;
  private totalWritten = 0;
  private totalDropped = 0;
  private capacity: number;
  /// 分片索引, 按 offset 递增
  private chunks: ChunkEntry[] = [];
  private listeners = new Set<() => void>();
  private statsListeners = new Set<(usage: number, length: number, capacity: number) => void>();
  /// RAF 节流标志
  private rafScheduled = false;
  /// 脏标记: 本帧内是否有新数据
  private dirty = false;

  constructor(capacity = 1_048_576) {
    this.capacity = capacity;
    this.buf = new Uint8Array(capacity);
  }

  /// 批量推入原始数据
  pushBatch(batch: RawDataBatch) {
    if (batch.chunks.length === 0 && batch.total_bytes === 0) return;
    this.totalDropped += batch.dropped_bytes ?? 0;
    for (const chunk of batch.chunks) {
      const bytes = chunk.bytes;
      if (bytes.length === 0) continue;
      const startOffset = this.totalWritten;
      for (let i = 0; i < bytes.length; i++) {
        this.buf[this.writePos] = bytes[i];
        this.writePos = (this.writePos + 1) % this.capacity;
        this.totalWritten++;
      }
      this.chunks.push({
        offset: startOffset,
        length: bytes.length,
        timestamp_us: chunk.timestamp_us,
      });
    }
    // 限制分片索引数量, 避免无限增长
    this.trimChunks();
    this.dirty = true;
    this.scheduleNotify();
  }

  /// 清理已被完全覆盖的分片元数据
  private trimChunks() {
    if (this.chunks.length <= 10000) return;
    const threshold = Math.max(0, this.totalWritten - this.capacity);
    let i = 0;
    while (i < this.chunks.length && this.chunks[i].offset + this.chunks[i].length <= threshold) {
      i++;
    }
    if (i > 0) {
      this.chunks = this.chunks.slice(i);
    }
  }

  /// 当前实际存储字节数
  get storedBytes(): number {
    return Math.min(this.totalWritten, this.capacity);
  }

  /// 总行数 (每 16 字节一行)
  get lineCount(): number {
    return Math.ceil(this.storedBytes / RAWDATA_BYTES_PER_ROW);
  }

  /// 获取指定行视图 (不复制底层字节)
  getLine(rowIndex: number): RawDataLineView {
    const stored = this.storedBytes;
    const baseOffset = Math.max(0, this.totalWritten - stored);
    const lineStart = baseOffset + rowIndex * RAWDATA_BYTES_PER_ROW;
    const lineEnd = Math.min(lineStart + RAWDATA_BYTES_PER_ROW, this.totalWritten);
    const length = Math.max(0, lineEnd - lineStart);

    const startPos = (this.writePos - stored + rowIndex * RAWDATA_BYTES_PER_ROW + this.capacity) % this.capacity;
    const bytes = new Uint8Array(length);
    for (let i = 0; i < length; i++) {
      bytes[i] = this.buf[(startPos + i) % this.capacity];
    }

    return {
      offset: lineStart,
      timestamp: this.getTimeForOffset(lineStart),
      bytes,
    };
  }

  /// 获取所有行 (仅用于导出/复制, 不建议在渲染循环中使用)
  getAllLines(): RawDataLineView[] {
    const count = this.lineCount;
    const lines: RawDataLineView[] = [];
    for (let i = 0; i < count; i++) {
      lines.push(this.getLine(i));
    }
    return lines;
  }

  /** 查找给定字节偏移对应的时间戳 (毫秒) */
  private getTimeForOffset(offset: number): number {
    if (this.chunks.length === 0) return 0;
    let lo = 0;
    let hi = this.chunks.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      const chunk = this.chunks[mid];
      if (offset >= chunk.offset && offset < chunk.offset + chunk.length) {
        return Math.floor(chunk.timestamp_us / 1000);
      }
      if (offset < chunk.offset) {
        hi = mid - 1;
      } else {
        lo = mid + 1;
      }
    }
    // 未精确命中则返回最近的前一个分片时间戳
    let candidate = this.chunks[0];
    for (const chunk of this.chunks) {
      if (chunk.offset <= offset) candidate = chunk;
      else break;
    }
    return Math.floor(candidate.timestamp_us / 1000);
  }

  /// 累计字节数 (含已丢弃)
  get totalBytes(): number {
    return this.totalWritten + this.totalDropped;
  }

  /// 累计丢弃字节数
  get droppedBytes(): number {
    return this.totalDropped;
  }

  /// 设置容量并保留最近数据
  setCapacity(newCapacity: number) {
    const cap = Math.max(1, newCapacity);
    if (cap === this.capacity) return;

    // 若当前存储量超过新容量, 丢弃最旧块
    while (this.storedBytes > cap && this.chunks.length > 0) {
      const front = this.chunks.shift();
      if (front) {
        this.totalDropped += front.length;
      }
    }

    // 重建 Uint8Array 并拷贝已有数据 (保持最近字节在前)
    const newBuf = new Uint8Array(cap);
    const stored = Math.min(this.storedBytes, cap);
    if (stored > 0) {
      const startPos = (this.writePos - this.storedBytes + this.capacity) % this.capacity;
      const offset = this.storedBytes - stored;
      for (let i = 0; i < stored; i++) {
        newBuf[i] = this.buf[(startPos + offset + i) % this.capacity];
      }
    }
    this.buf = newBuf;
    this.writePos = stored % cap;
    this.totalWritten = stored;
    this.capacity = cap;
    this.dirty = true;
    this.scheduleNotify();
  }

  clear() {
    this.buf.fill(0);
    this.writePos = 0;
    this.totalWritten = 0;
    this.totalDropped = 0;
    this.chunks = [];
    this.dirty = true;
    this.scheduleNotify();
  }

  /// 订阅数据变化 (RAF 节流后触发, 无参数, 调用方自行读取行)
  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /// 订阅缓存使用量统计, usage ∈ [0,1]
  subscribeStats(fn: (usage: number, length: number, capacity: number) => void): () => void {
    this.statsListeners.add(fn);
    fn(this.storedBytes / this.capacity, this.storedBytes, this.capacity);
    return () => this.statsListeners.delete(fn);
  }

  /// 立即触发一次通知 (兼容旧代码入口, 实际已被 RAF 节流替代)
  notify() {
    this.scheduleNotify();
  }

  /// RAF 节流: 同一帧内多次 push 合并为一次通知
  private scheduleNotify() {
    if (this.rafScheduled) return;
    this.rafScheduled = true;
    requestAnimationFrame(() => {
      this.rafScheduled = false;
      this.flushNotify();
    });
  }

  private flushNotify() {
    if (!this.dirty) return;
    this.dirty = false;
    this.listeners.forEach((fn) => fn());

    const stored = this.storedBytes;
    const usage = stored / this.capacity;
    this.statsListeners.forEach((fn) => fn(usage, stored, this.capacity));
  }
}

export const rawDataBuffer = new RawDataBuffer();

/// 兼容旧代码的导出 (DataFrame 类型仍需要)
export type { DataFrame };
