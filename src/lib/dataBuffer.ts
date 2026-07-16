import { DataFrame } from '../types';

/// 波形数据缓冲区 — 模块级单例, 避免频繁触发 React re-render
export class WaveformBuffer {
  private timestamps: number[] = [];
  private channels: number[][] = [];
  private maxPoints: number;
  private numChannels: number;
  private listeners = new Set<() => void>();
  /// 版本号: 每次 push 递增, 用于检测数据变化 (即使 pointCount 因满而不再增长)
  private _version = 0;

  constructor(maxPoints = 10000, numChannels = 8) {
    this.maxPoints = maxPoints;
    this.numChannels = numChannels;
    this.channels = Array.from({ length: numChannels }, () => []);
  }

  push(frame: DataFrame) {
    this.timestamps.push(frame.timestamp);
    for (let i = 0; i < this.numChannels; i++) {
      this.channels[i].push(frame.channels[i] ?? 0);
    }
    if (this.timestamps.length > this.maxPoints) {
      const cut = this.timestamps.length - this.maxPoints;
      this.timestamps.splice(0, cut);
      for (let i = 0; i < this.numChannels; i++) {
        this.channels[i].splice(0, cut);
      }
    }
    this._version++;
    this.notify();
  }

  /** 获取 uPlot 格式数据: [timestamps, ch0, ch1, ...] */
  getData(): number[][] {
    return [this.timestamps, ...this.channels];
  }

  get channelCount(): number {
    return this.numChannels;
  }

  get pointCount(): number {
    return this.timestamps.length;
  }

  /** 版本号: 每次推送递增, 不受 maxPoints 截断影响 */
  get version(): number {
    return this._version;
  }

  setChannels(num: number) {
    this.numChannels = num;
    this.channels = Array.from({ length: num }, () => []);
    this.timestamps = [];
  }

  clear() {
    this.timestamps = [];
    this.channels = Array.from({ length: this.numChannels }, () => []);
    this._version = 0;
    this.notify();
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  notify() {
    this.listeners.forEach((fn) => fn());
  }

  /// 注入测试数据 (仅用于前端调试) — 生成 count 个采样点的正弦波
  injectTestData(count: number) {
    this.clear();
    const channels = this.numChannels || 4;
    for (let n = 0; n < count; n++) {
      const t = n * 0.05;
      const channelsArr: number[] = [];
      for (let c = 0; c < channels; c++) {
        const freq = 1 + c * 0.5;
        channelsArr.push(Math.sin(t * freq) * (50 + c * 20) + 128);
      }
      this.push({ timestamp: t, channels: channelsArr });
    }
  }
}

/// 全局波形缓冲区实例
export const waveformBuffer = new WaveformBuffer();

/// 原始数据缓冲区 — 环形缓冲区, 支持时间戳追踪
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
