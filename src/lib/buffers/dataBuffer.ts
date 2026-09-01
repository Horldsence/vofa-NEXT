import type {
  DataFrame,
  RawDataBatch,
  RawDataDirection,
  WaveformWindow,
  WaveformWindowPayload,
} from '../../types';
import { createFrameBatcher } from '../utils/frameBatcher';

export const RAWDATA_BYTES_PER_ROW = 16;

/// base64 → Uint8Array (atob 一次解码 + 定长拷贝, ~100MB/s 量级)
function decodeBase64(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) {
    out[i] = bin.charCodeAt(i);
  }
  return out;
}

/// 原始数据单行视图
export interface RawDataLineView {
  offset: number;
  timestamp: number;
  direction: RawDataDirection;
  bytes: Uint8Array;
}

/// 原始数据快照 (兼容旧接口, 当前主要由虚拟列表直接按行读取)
export interface RawDataSnapshot {
  lines: RawDataLineView[];
  totalBytes: number;
}

let nextWaveformCacheVersion = 1;

function normalizeSamples(values: (number | null)[]): number[] {
  return values.map((value) => value ?? NaN);
}

/** 把 JSON 中的 null 缺口统一恢复为渲染层使用的 NaN。 */
export function normalizeWaveformWindow(payload: WaveformWindowPayload): WaveformWindow {
  const derived: WaveformWindow['derived'] = {};
  for (const [sinkId, sources] of Object.entries(payload.derived)) {
    const normalizedSources: Record<string, Record<string, number[]>> = {};
    for (const [sourceId, handles] of Object.entries(sources)) {
      const normalizedHandles: Record<string, number[]> = {};
      for (const [sourceHandle, values] of Object.entries(handles)) {
        normalizedHandles[sourceHandle] = normalizeSamples(values);
      }
      normalizedSources[sourceId] = normalizedHandles;
    }
    derived[sinkId] = normalizedSources;
  }
  return {
    ...payload,
    channels: payload.channels.map(normalizeSamples),
    derived,
  };
}

/// 波形窗口缓存 — 接收来自后端 Tauri Channel 的推送, 由订阅者维护
/// 不同于旧的 WaveformBuffer (前端持有完整数据), 此处仅缓存最新窗口快照
export class WaveformWindowCache {
  private latest: WaveformWindow = emptyWaveformWindow();
  private _version = 0;
  private listeners = new Set<() => void>();
  private statsListeners = new Set<(usage: number, length: number, capacity: number) => void>();
  /// 统计通知按帧合并: 同一帧内多次 set/clear 只通知一次, 避免状态栏 30FPS+ 重渲染
  private statsBatcher = createFrameBatcher<{ usage: number; length: number; capacity: number }>(
    (s) => this.flushStats(s.usage, s.length, s.capacity)
  );

  set(window: WaveformWindow) {
    this.latest = window;
    this._version = nextWaveformCacheVersion++;
    this.notify();
    this.statsBatcher.push(this.currentStats());
  }

  get(): WaveformWindow {
    return this.latest;
  }

  get version(): number {
    return this._version;
  }

  clear() {
    this.latest = emptyWaveformWindow();
    this._version = nextWaveformCacheVersion++;
    this.notify();
    this.statsBatcher.push(this.currentStats());
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /// 订阅波形缓存使用率统计, usage ∈ [0,1]
  subscribeStats(fn: (usage: number, length: number, capacity: number) => void): () => void {
    this.statsListeners.add(fn);
    const stats = this.currentStats();
    fn(stats.usage, stats.length, stats.capacity);
    return () => this.statsListeners.delete(fn);
  }

  private notify() {
    this.listeners.forEach((fn) => fn());
  }

  private currentStats() {
    const capacity = Math.max(1, this.latest.buffer_capacity);
    const length = this.latest.buffer_points;
    return { usage: length / capacity, length, capacity };
  }

  private flushStats(usage: number, length: number, capacity: number) {
    this.statsListeners.forEach((fn) => fn(usage, length, capacity));
  }
}

function emptyWaveformWindow(): WaveformWindow {
  return {
    seq: 0,
    timestamps: [],
    channels: [],
    channel_count: 0,
    derived: {},
    buffer_points: 0,
    buffer_capacity: 1,
    latest_timestamp_us: 0,
    raw_window_points: 0,
    sampling: 'raw',
  };
}

/// 全局波形窗口缓存
export const waveformWindow = new WaveformWindowCache();

/// 分片元数据 — 用于把字节偏移映射到时间戳与方向
export interface ChunkEntry {
  /// 该分片第一个字节在全局字节流中的偏移
  offset: number;
  /// 分片长度 (字节)
  length: number;
  /// 微秒时间戳
  timestamp_us: number;
  /// 数据方向
  direction: RawDataDirection;
}

/// 原始数据行读取接口 — RawDataView 渲染所需的最小契约
/// RawDataBuffer (全量) 与 FilteredRawDataBuffer (方向/搜索过滤) 均实现
export interface RawDataLineSource {
  /// 网格模式行数 (每 16 字节一行)
  readonly lineCount: number;
  /// 换行模式行数 (0x0A 分隔)
  readonly newlineLineCount: number;
  /// 获取网格模式指定行
  getLine(rowIndex: number): RawDataLineView;
  /// 获取换行模式指定行
  getNewlineLine(rowIndex: number): RawDataLineView;
  /// 累计字节数 (含已丢弃)
  readonly totalBytes: number;
  /// 累计丢弃字节数
  readonly droppedBytes: number;
  /// 订阅数据变化 (RAF 节流后触发)
  subscribe(fn: () => void): () => void;
  /// 清空视图 (过滤缓冲只清自身索引, 不影响源)
  clear(): void;
}

/// 原始数据缓冲区 — 基于 Uint8Array 的环形缓冲区
/// 接收来自后端 subscribe_data 统一入口 rawdata 通道的 RawDataBatch, RAF 节流后通知订阅者
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
  /// 换行索引: 每个元素的值为换行行的起始绝对流偏移 (0x0A 作为行末字节被包含)
  private lineStarts: number[] = [];
  /// 增量扫描已覆盖到的最大绝对偏移
  private lastScannedOffset = 0;
  /// 脏标记: 容量变更后需全量重建换行索引
  private lineIndexDirty = false;

  constructor(capacity = 1_048_576) {
    this.capacity = capacity;
    this.buf = new Uint8Array(capacity);
  }

  /// 批量推入原始数据 (base64 解码 + 环形块拷贝, 支持 7MB/s+ 吞吐)
  pushBatch(batch: RawDataBatch) {
    if (batch.chunks.length === 0 && batch.total_bytes === 0) return;
    this.totalDropped += batch.dropped_bytes ?? 0;
    for (const chunk of batch.chunks) {
      const bytes = decodeBase64(chunk.bytes_b64);
      const n = bytes.length;
      if (n === 0) continue;
      const startOffset = this.totalWritten;
      if (n >= this.capacity) {
        // 单块即超过容量: 只保留尾部 capacity 字节, 写满整个环
        this.buf.set(bytes.subarray(n - this.capacity), 0);
        this.writePos = 0;
        this.totalWritten += n;
      } else {
        const first = Math.min(n, this.capacity - this.writePos);
        this.buf.set(bytes.subarray(0, first), this.writePos);
        if (first < n) {
          this.buf.set(bytes.subarray(first), 0);
        }
        this.writePos = (this.writePos + n) % this.capacity;
        this.totalWritten += n;
      }
      this.chunks.push({
        offset: startOffset,
        length: n,
        timestamp_us: chunk.timestamp_us,
        direction: chunk.direction ?? 'rx',
      });
    }
    // 限制分片索引数量, 避免无限增长
    this.trimChunks();
    this.updateLineIndex();
    this.dirty = true;
    this.scheduleNotify();
  }

  /// 清理已被完全覆盖的分片元数据
  private trimChunks() {
    if (this.chunks.length <= 2000) return;
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

  get capacityBytes(): number {
    return this.capacity;
  }

  /// 累计写入字节数 (绝对偏移空间; 最早保留偏移 = max(0, writtenTotal - storedBytes))
  get writtenTotal(): number {
    return this.totalWritten;
  }

  /// 分片元数据 (只读引用, 按 offset 递增) — 供过滤视图做增量索引
  /// 调用方不得修改返回数组及其元素
  getChunkEntries(): readonly ChunkEntry[] {
    return this.chunks;
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
      direction: this.getDirectionForOffset(lineStart),
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

  /// 复制绝对偏移 [startOffset, endOffset) 区间的环形字节 (定位方式与 getLine 一致)
  /// 调用方需保证区间落在当前保留窗口内 (startOffset >= writtenTotal - storedBytes)
  readBytesAt(startOffset: number, endOffset: number): Uint8Array {
    const length = Math.max(0, endOffset - startOffset);
    const bytes = new Uint8Array(length);
    if (length === 0) return bytes;
    const stored = this.storedBytes;
    const baseOffset = Math.max(0, this.totalWritten - stored);
    const startPos = (this.writePos - stored + (startOffset - baseOffset) + this.capacity) % this.capacity;
    for (let i = 0; i < length; i++) {
      bytes[i] = this.buf[(startPos + i) % this.capacity];
    }
    return bytes;
  }

  /// 增量维护换行索引 — 只扫描新写入的字节 (pushBatch 后调用)
  private updateLineIndex() {
    const stored = this.storedBytes;
    const baseOffset = Math.max(0, this.totalWritten - stored);

    while (this.lineStarts.length > 0 && this.lineStarts[0] < baseOffset) {
      this.lineStarts.shift();
    }
    if (this.lineStarts.length === 0 || this.lineStarts[0] !== baseOffset) {
      this.lineStarts.unshift(baseOffset);
    }

    const scanStart = Math.max(this.lastScannedOffset, baseOffset);
    const scanCount = Math.max(0, this.totalWritten - scanStart);
    const startPos = (this.writePos - stored + (scanStart - baseOffset) + this.capacity) % this.capacity;
    for (let i = 0; i < scanCount; i++) {
      if (this.buf[(startPos + i) % this.capacity] === 0x0a) {
        this.lineStarts.push(scanStart + i + 1);
      }
    }

    this.lastScannedOffset = this.totalWritten;
  }

  /// 全量重建换行索引 (仅在容量变更后惰性调用)
  private rebuildLineIndex() {
    this.lineStarts = [];
    this.lastScannedOffset = this.totalWritten;
    this.lineIndexDirty = false;
    const stored = this.storedBytes;
    const baseOffset = Math.max(0, this.totalWritten - stored);
    if (stored === 0) return;
    this.lineStarts.push(baseOffset);
    const startPos = (this.writePos - stored + this.capacity) % this.capacity;
    for (let i = 0; i < stored; i++) {
      if (this.buf[(startPos + i) % this.capacity] === 0x0a) {
        this.lineStarts.push(baseOffset + i + 1);
      }
    }
  }

  /// 换行模式行数 (0x0A 分隔; 换行符作为行末字节, 空行计 1 行)
  get newlineLineCount(): number {
    if (this.lineIndexDirty) this.rebuildLineIndex();
    return this.lineStarts.length;
  }

  /// 获取换行模式下的指定行视图 (行跨 [lineStarts[i], lineStarts[i+1]))
  getNewlineLine(rowIndex: number): RawDataLineView {
    if (this.lineIndexDirty) this.rebuildLineIndex();
    if (rowIndex < 0 || rowIndex >= this.lineStarts.length) {
      return { offset: this.totalWritten, timestamp: this.getTimeForOffset(this.totalWritten), direction: 'rx', bytes: new Uint8Array(0) };
    }
    const start = this.lineStarts[rowIndex];
    const end = rowIndex + 1 < this.lineStarts.length ? this.lineStarts[rowIndex + 1] : this.totalWritten;
    return {
      offset: start,
      timestamp: this.getTimeForOffset(start),
      direction: this.getDirectionForOffset(start),
      bytes: this.readBytesAt(start, end),
    };
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

  /** 查找给定字节偏移对应的数据方向 */
  private getDirectionForOffset(offset: number): RawDataDirection {
    if (this.chunks.length === 0) return 'rx';
    let lo = 0;
    let hi = this.chunks.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      const chunk = this.chunks[mid];
      if (offset >= chunk.offset && offset < chunk.offset + chunk.length) {
        return chunk.direction;
      }
      if (offset < chunk.offset) {
        hi = mid - 1;
      } else {
        lo = mid + 1;
      }
    }
    // 未精确命中则返回最近的前一个分片方向
    let candidate = this.chunks[0];
    for (const chunk of this.chunks) {
      if (chunk.offset <= offset) candidate = chunk;
      else break;
    }
    return candidate.direction;
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
    this.lineIndexDirty = true;
    this.dirty = true;
    this.scheduleNotify();
  }

  clear() {
    this.buf.fill(0);
    this.writePos = 0;
    this.totalWritten = 0;
    this.totalDropped = 0;
    this.chunks = [];
    this.lineStarts = [];
    this.lastScannedOffset = 0;
    this.lineIndexDirty = false;
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
