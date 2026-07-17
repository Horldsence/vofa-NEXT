import type { DataFrame, WaveformWindow } from '../types';

/// 波形窗口缓存 — 接收来自后端 Tauri Channel 的推送, 由订阅者维护
/// 不同于旧的 WaveformBuffer (前端持有完整数据), 此处仅缓存最新窗口快照
class WaveformWindowCache {
  private latest: WaveformWindow = { timestamps: [], channels: [], channel_count: 0 };
  private _version = 0;
  private listeners = new Set<() => void>();

  set(window: WaveformWindow) {
    this.latest = window;
    this._version++;
    this.notify();
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
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private notify() {
    this.listeners.forEach((fn) => fn());
  }
}

/// 全局波形窗口缓存
export const waveformWindow = new WaveformWindowCache();

/// 原始数据缓冲区 — 环形缓冲区, 支持时间戳追踪 (前端仍需保留以格式化 hex/ascii)
export class RawDataBuffer {
  private buf: Uint8Array;
  private writePos = 0;
  private totalWritten = 0;
  private capacity: number;
  /// 时间戳条目: 记录每次 push 的起始字节偏移和时间
  private timeEntries: { offset: number; time: number }[] = [];
  private listeners = new Set<(data: RawDataSnapshot) => void>();

  constructor(capacity = 65536) {
    this.capacity = capacity;
    this.buf = new Uint8Array(capacity);
  }

  push(data: number[] | Uint8Array) {
    const now = Date.now();
    this.timeEntries.push({ offset: this.totalWritten, time: now });
    // 限制时间戳条目数量
    if (this.timeEntries.length > 1000) {
      this.timeEntries.splice(0, this.timeEntries.length - 1000);
    }
    for (let i = 0; i < data.length; i++) {
      this.buf[this.writePos] = data[i];
      this.writePos = (this.writePos + 1) % this.capacity;
      this.totalWritten++;
    }
  }

  /** 查找给定字节偏移对应的时间戳 */
  private getTimeForOffset(offset: number): number | null {
    if (this.timeEntries.length === 0) return null;
    // 二分查找最大的 timeEntry.offset <= offset
    let lo = 0, hi = this.timeEntries.length - 1, result = null;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (this.timeEntries[mid].offset <= offset) {
        result = this.timeEntries[mid];
        lo = mid + 1;
      } else {
        hi = mid - 1;
      }
    }
    return result ? result.time : this.timeEntries[0].time;
  }

  /** 获取最近的 N 个字节, 格式化为 hex + ascii + 时间戳 */
  getRecentLines(maxBytes = 4096): RawDataSnapshot {
    const count = Math.min(maxBytes, this.totalWritten);
    const bytes: number[] = [];
    const startPos = (this.writePos - count + this.capacity) % this.capacity;
    for (let i = 0; i < count; i++) {
      bytes.push(this.buf[(startPos + i) % this.capacity]);
    }

    const baseOffset = Math.max(0, this.totalWritten - count);

    // 格式化为 hex + ascii + 时间戳, 每 16 字节一行
    const lines: string[] = [];
    const asciiLines: string[] = [];
    const timestamps: number[] = [];
    for (let i = 0; i < bytes.length; i += 16) {
      const chunk = bytes.slice(i, i + 16);
      const hex = chunk.map((b) => b.toString(16).padStart(2, '0')).join(' ');
      const ascii = chunk.map((b) => (b >= 32 && b < 127) ? String.fromCharCode(b) : '.').join('');
      lines.push(hex.padEnd(47, ' '));
      asciiLines.push(ascii);
      timestamps.push(this.getTimeForOffset(baseOffset + i) ?? 0);
    }

    return {
      hex: lines.join('\n'),
      ascii: asciiLines.join('\n'),
      timestamps,
      offset: baseOffset,
    };
  }

  get totalBytes(): number {
    return this.totalWritten;
  }

  clear() {
    this.buf.fill(0);
    this.writePos = 0;
    this.totalWritten = 0;
    this.timeEntries = [];
  }

  subscribe(fn: (data: RawDataSnapshot) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  notify() {
    const data = this.getRecentLines();
    this.listeners.forEach((fn) => fn(data));
  }
}

export interface RawDataSnapshot {
  hex: string;
  ascii: string;
  timestamps: number[];
  offset: number;
}

export const rawDataBuffer = new RawDataBuffer();

/// 兼容旧代码的导出 (DataFrame 类型仍需要)
export type { DataFrame };
