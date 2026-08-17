import {
  RawDataBuffer,
  RAWDATA_BYTES_PER_ROW,
  type RawDataLineSource,
  type RawDataLineView,
} from './dataBuffer';
import type { RawDataDirection } from '../../types';

/// 方向过滤条件 — 'all' 不做方向过滤
export type RawDirectionFilter = 'all' | RawDataDirection;

/// 已索引的匹配分片 — 过滤流偏移 ↔ 源流偏移 的映射
interface IndexedChunk {
  /// 源流绝对偏移 [rawStart, rawEnd)
  rawStart: number;
  rawEnd: number;
  /// 该分片首字节在过滤流中的偏移
  filteredStart: number;
  timestamp_us: number;
  direction: RawDataDirection;
}

/// 解析搜索词为字节模式 — 语义与后端 SearchPattern::parse 一致:
/// - 空串/纯空白 → null (不过滤)
/// - 只含十六进制字符与空白 → 按 hex 解析 (支持 `31 32` 或 `3132`)
/// - 其他 → 按 UTF-8 字符串解析
export function parseSearchPattern(input: string): Uint8Array | null {
  const trimmed = input.trim();
  if (!trimmed) return null;
  if (/^[0-9a-fA-F\s]+$/.test(trimmed)) {
    const bytes: number[] = [];
    for (const token of trimmed.split(/\s+/)) {
      for (let i = 0; i < token.length; i += 2) {
        const v = parseInt(token.slice(i, i + 2), 16);
        if (Number.isNaN(v)) return null;
        bytes.push(v);
      }
    }
    if (bytes.length > 0) return new Uint8Array(bytes);
  }
  return new TextEncoder().encode(trimmed);
}

/// 子串匹配 (单字节与多字节分开处理, 避免 windows 分配)
function containsBytes(haystack: Uint8Array, needle: Uint8Array): boolean {
  if (needle.length === 0) return true;
  if (haystack.length < needle.length) return false;
  const first = needle[0];
  const limit = haystack.length - needle.length;
  for (let i = 0; i <= limit; i++) {
    if (haystack[i] !== first) continue;
    let j = 1;
    while (j < needle.length && haystack[i + j] === needle[j]) j++;
    if (j === needle.length) return true;
  }
  return false;
}

/// 过滤原始数据缓冲 — 对源 RawDataBuffer 做增量方向/搜索过滤的只读视图
///
/// 设计动机: 前端全局 buffer 与后端 collector 保留相同窗口, 过滤无需第二条
/// IPC 流 (双倍传输曾压垮主线程)。本类复用源 buffer 的既有数据:
/// - 方向过滤: 按 chunk 元数据筛选, 渲染时按过滤流偏移映射回源环形缓冲
/// - 搜索过滤: 增量扫描新到达的匹配方向字节 (跨 chunk 尾部拼接)
/// - 换行索引: 只对匹配字节增量维护, 源环覆盖时同步裁剪
///
/// 实现 RawDataLineSource, 视图层无需区分全量/过滤 buffer。
export class FilteredRawDataBuffer implements RawDataLineSource {
  private chunks: IndexedChunk[] = [];
  /// 过滤流累计字节 (含已被源环覆盖丢弃的)
  private filteredTotal = 0;
  /// 当前最早保留字节的过滤流偏移
  private filteredBase = 0;
  /// 被源环覆盖而丢弃的匹配字节数
  private dropped = 0;
  /// 已处理到的源绝对偏移
  private lastRawOffset: number;
  /// 跨 chunk 搜索尾部 (上一个方向匹配 chunk 的末尾 pattern.len-1 字节)
  private prevTail = new Uint8Array(0);
  /// 换行索引: 过滤流偏移 (0x0A 为行末)
  private lineStarts: number[] = [];
  /// lineStarts 有效头部 (避免 shift 的 O(n) 重排)
  private lineHead = 0;
  private listeners = new Set<() => void>();
  private readonly unsubSource: () => void;

  constructor(
    private readonly source: RawDataBuffer,
    private readonly direction: RawDirectionFilter,
    private readonly pattern: Uint8Array | null
  ) {
    // 从源当前最早保留偏移开始索引 (即回放全部保留历史)
    this.lastRawOffset = Math.max(0, source.writtenTotal - source.storedBytes);
    this.update();
    this.unsubSource = source.subscribe(() => {
      this.update();
      this.listeners.forEach((fn) => fn());
    });
  }

  /// 断开与源 buffer 的订阅 (视图卸载/切换过滤条件时调用)
  dispose(): void {
    this.unsubSource();
  }

  private directionMatches(d: RawDataDirection): boolean {
    return this.direction === 'all' || d === this.direction;
  }

  /// 增量索引: 裁剪被源环覆盖的旧数据 + 摄入新分片
  private update(): void {
    const base = Math.max(0, this.source.writtenTotal - this.source.storedBytes);

    // 源环在后台 (RAF 暂停) 可能已绕过处理点: 跳到最早保留处, 重置跨 chunk 尾部
    if (this.lastRawOffset < base) {
      this.lastRawOffset = base;
      this.prevTail = new Uint8Array(0);
    }

    // 1. 裁剪: 丢弃完全被覆盖的分片, 截断部分覆盖的首分片
    while (this.chunks.length > 0 && this.chunks[0].rawEnd <= base) {
      const c = this.chunks[0];
      this.dropped += c.rawEnd - c.rawStart;
      this.filteredBase = c.filteredStart + (c.rawEnd - c.rawStart);
      this.chunks.shift();
    }
    if (this.chunks.length > 0 && this.chunks[0].rawStart < base) {
      const c = this.chunks[0];
      const cut = base - c.rawStart;
      c.rawStart += cut;
      c.filteredStart += cut;
      this.dropped += cut;
      this.filteredBase = c.filteredStart;
    }
    // 换行索引同步裁剪: 覆盖点落入行中时, 首行从 filteredBase 起算
    while (this.lineHead < this.lineStarts.length && this.lineStarts[this.lineHead] < this.filteredBase) {
      this.lineHead++;
    }
    if (
      this.lineHead > 0 &&
      this.filteredTotal > this.filteredBase &&
      (this.lineHead === this.lineStarts.length || this.lineStarts[this.lineHead] > this.filteredBase)
    ) {
      this.lineStarts[this.lineHead - 1] = this.filteredBase;
      this.lineHead--;
    }
    if (this.lineHead > 4096 && this.lineHead * 2 > this.lineStarts.length) {
      this.lineStarts = this.lineStarts.slice(this.lineHead);
      this.lineHead = 0;
    }

    // 2. 摄入: 处理新分片 (分片按 offset 递增)
    const entries = this.source.getChunkEntries();
    for (const e of entries) {
      const eEnd = e.offset + e.length;
      if (eEnd <= this.lastRawOffset) continue;
      const s = Math.max(e.offset, this.lastRawOffset);
      this.lastRawOffset = eEnd;
      if (!this.directionMatches(e.direction)) continue;

      const bytes = this.source.readBytesAt(s, eEnd);
      let match = true;
      if (this.pattern && this.pattern.length > 0) {
        match = containsBytes(concatBytes(this.prevTail, bytes), this.pattern);
        const keep = this.pattern.length - 1;
        this.prevTail = keep > 0 ? bytes.slice(Math.max(0, bytes.length - keep)) : new Uint8Array(0);
      }
      if (!match) continue;

      const filteredStart = this.filteredTotal;
      this.chunks.push({
        rawStart: s,
        rawEnd: eEnd,
        filteredStart,
        timestamp_us: e.timestamp_us,
        direction: e.direction,
      });
      // 首个匹配分片: 首行从过滤流起点开始
      if (this.lineHead === this.lineStarts.length) this.lineStarts.push(filteredStart);
      for (let i = 0; i < bytes.length; i++) {
        if (bytes[i] === 0x0a) this.lineStarts.push(filteredStart + i + 1);
      }
      this.filteredTotal += bytes.length;
    }
  }

  /// 过滤流当前保留字节数
  get storedBytes(): number {
    return this.filteredTotal - this.filteredBase;
  }

  get lineCount(): number {
    return Math.ceil(this.storedBytes / RAWDATA_BYTES_PER_ROW);
  }

  get newlineLineCount(): number {
    return this.lineStarts.length - this.lineHead;
  }

  get totalBytes(): number {
    return this.filteredTotal;
  }

  get droppedBytes(): number {
    return this.dropped;
  }

  getLine(rowIndex: number): RawDataLineView {
    const start = this.filteredBase + rowIndex * RAWDATA_BYTES_PER_ROW;
    const end = Math.min(start + RAWDATA_BYTES_PER_ROW, this.filteredTotal);
    return this.readFilteredRange(start, end);
  }

  getNewlineLine(rowIndex: number): RawDataLineView {
    const i = this.lineHead + rowIndex;
    if (rowIndex < 0 || i >= this.lineStarts.length) {
      return { offset: this.filteredTotal, timestamp: 0, direction: 'rx', bytes: new Uint8Array(0) };
    }
    const start = this.lineStarts[i];
    const end = i + 1 < this.lineStarts.length ? this.lineStarts[i + 1] : this.filteredTotal;
    return this.readFilteredRange(start, end);
  }

  /// 按过滤流偏移区间读取 (二分定位分片, 映射回源环拷贝)
  private readFilteredRange(start: number, end: number): RawDataLineView {
    if (end <= start) {
      return { offset: start, timestamp: 0, direction: 'rx', bytes: new Uint8Array(0) };
    }
    const out = new Uint8Array(end - start);
    let written = 0;
    let timestamp = 0;
    let direction: RawDataDirection = 'rx';

    // 二分: 第一个 filteredEnd > start 的分片
    let lo = 0;
    let hi = this.chunks.length - 1;
    let idx = this.chunks.length;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      const c = this.chunks[mid];
      if (c.filteredStart + (c.rawEnd - c.rawStart) > start) {
        idx = mid;
        hi = mid - 1;
      } else {
        lo = mid + 1;
      }
    }
    for (let i = idx; i < this.chunks.length && written < out.length; i++) {
      const c = this.chunks[i];
      const cLen = c.rawEnd - c.rawStart;
      const s = Math.max(start, c.filteredStart);
      const e = Math.min(end, c.filteredStart + cLen);
      if (e <= s) continue;
      const part = this.source.readBytesAt(c.rawStart + (s - c.filteredStart), c.rawStart + (e - c.filteredStart));
      out.set(part, written);
      if (written === 0) {
        timestamp = Math.floor(c.timestamp_us / 1000);
        direction = c.direction;
      }
      written += part.length;
    }
    return {
      offset: start,
      timestamp,
      direction,
      bytes: written === out.length ? out : out.slice(0, written),
    };
  }

  /// 清空过滤视图 (不影响源 buffer); 从源当前最新处重新开始
  clear(): void {
    this.chunks = [];
    this.filteredTotal = 0;
    this.filteredBase = 0;
    this.dropped = 0;
    this.lastRawOffset = this.source.writtenTotal;
    this.prevTail = new Uint8Array(0);
    this.lineStarts = [];
    this.lineHead = 0;
    this.listeners.forEach((fn) => fn());
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }
}

/// 拼接两个字节数组 (跨 chunk 搜索用, 长度为 tail + chunk)
function concatBytes(a: Uint8Array, b: Uint8Array): Uint8Array {
  if (a.length === 0) return b;
  const out = new Uint8Array(a.length + b.length);
  out.set(a, 0);
  out.set(b, a.length);
  return out;
}
