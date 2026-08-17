import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { RawDataBuffer } from '../buffers/dataBuffer';
import { FilteredRawDataBuffer, parseSearchPattern } from '../buffers/filteredRawDataBuffer';
import type { RawDataDirection } from '../../types';

/// 手工 rAF ticker — 让源 buffer 的帧级节流完全确定
function installManualRaf() {
  const origRaf = globalThis.requestAnimationFrame;
  const origCaf = globalThis.cancelAnimationFrame;
  let queued: { id: number; cb: FrameRequestCallback }[] = [];
  let nextId = 1;
  globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
    const id = nextId++;
    queued.push({ id, cb });
    return id;
  }) as typeof requestAnimationFrame;
  globalThis.cancelAnimationFrame = ((id: number) => {
    queued = queued.filter((q) => q.id !== id);
  }) as typeof cancelAnimationFrame;
  return {
    flush() {
      const items = queued.splice(0);
      for (const { cb } of items) cb(0);
    },
    restore() {
      globalThis.requestAnimationFrame = origRaf;
      globalThis.cancelAnimationFrame = origCaf;
    },
  };
}

function toB64(bytes: number[] | Uint8Array): string {
  return btoa(String.fromCharCode(...bytes));
}

let seq = 0;
/// 向源 buffer 推入一个分片
function push(src: RawDataBuffer, direction: RawDataDirection, bytes: number[], timestamp_us = 0) {
  src.pushBatch({
    seq: seq++,
    chunks: [{ timestamp_us, direction, bytes_b64: toB64(bytes) }],
    total_bytes: bytes.length,
    dropped_bytes: 0,
  });
}

function bytes(n: number, start = 0): number[] {
  return Array.from({ length: n }, (_, i) => (start + i) & 0xff);
}

let ticker: ReturnType<typeof installManualRaf>;

beforeEach(() => {
  ticker = installManualRaf();
  seq = 0;
});

afterEach(() => {
  ticker.restore();
});

describe('parseSearchPattern', () => {
  it('空串/纯空白 → null', () => {
    expect(parseSearchPattern('')).toBeNull();
    expect(parseSearchPattern('   ')).toBeNull();
  });

  it('纯 hex → 字节解析 (支持空格分组与奇数尾)', () => {
    expect(Array.from(parseSearchPattern('deadbeef')!)).toEqual([0xde, 0xad, 0xbe, 0xef]);
    expect(Array.from(parseSearchPattern('DE AD')!)).toEqual([0xde, 0xad]);
    expect(Array.from(parseSearchPattern('abc')!)).toEqual([0xab, 0x0c]);
  });

  it('非 hex → UTF-8 字节', () => {
    expect(Array.from(parseSearchPattern('hello')!)).toEqual([0x68, 0x65, 0x6c, 0x6c, 0x6f]);
    expect(Array.from(parseSearchPattern('zz')!)).toEqual([0x7a, 0x7a]);
  });
});

describe('FilteredRawDataBuffer 方向过滤', () => {
  it('rx 过滤: 只保留 rx 分片, 构造时回放保留历史', () => {
    const src = new RawDataBuffer(1024);
    push(src, 'rx', bytes(16, 0));
    push(src, 'tx', bytes(16, 100));
    push(src, 'rx', bytes(16, 16));
    ticker.flush();

    const f = new FilteredRawDataBuffer(src, 'rx', null);
    expect(f.storedBytes).toBe(32);
    expect(f.lineCount).toBe(2);
    expect(Array.from(f.getLine(0).bytes)).toEqual(bytes(16, 0));
    expect(Array.from(f.getLine(1).bytes)).toEqual(bytes(16, 16));
    expect(f.getLine(0).direction).toBe('rx');
    f.dispose();
  });

  it('tx 过滤 / all 不过滤', () => {
    const src = new RawDataBuffer(1024);
    push(src, 'rx', bytes(16, 0));
    push(src, 'tx', bytes(8, 200));
    ticker.flush();

    const fTx = new FilteredRawDataBuffer(src, 'tx', null);
    expect(fTx.storedBytes).toBe(8);
    expect(Array.from(fTx.getLine(0).bytes)).toEqual(bytes(8, 200));
    fTx.dispose();

    const fAll = new FilteredRawDataBuffer(src, 'all', null);
    expect(fAll.storedBytes).toBe(24);
    fAll.dispose();
  });

  it('增量更新: 新分片经 RAF 通知后进入过滤视图', () => {
    const src = new RawDataBuffer(1024);
    const f = new FilteredRawDataBuffer(src, 'rx', null);
    expect(f.lineCount).toBe(0);

    push(src, 'rx', bytes(16, 0));
    push(src, 'tx', bytes(16, 50));
    ticker.flush();
    expect(f.lineCount).toBe(1);
    expect(Array.from(f.getLine(0).bytes)).toEqual(bytes(16, 0));
    f.dispose();
  });
});

describe('FilteredRawDataBuffer 搜索过滤', () => {
  it('hex 搜索: 仅包含模式的分片进入视图', () => {
    const src = new RawDataBuffer(1024);
    push(src, 'rx', [0xde, 0xad, 0xbe, 0xef]);
    push(src, 'rx', [0x00, 0x11, 0x22, 0x33]);
    ticker.flush();

    const f = new FilteredRawDataBuffer(src, 'all', parseSearchPattern('DE AD'));
    expect(f.storedBytes).toBe(4);
    expect(Array.from(f.getLine(0).bytes)).toEqual([0xde, 0xad, 0xbe, 0xef]);
    f.dispose();
  });

  it('ascii 搜索', () => {
    const src = new RawDataBuffer(1024);
    push(src, 'rx', Array.from(new TextEncoder().encode('xxhelloyy')));
    push(src, 'rx', [0x00, 0x01]);
    ticker.flush();

    const f = new FilteredRawDataBuffer(src, 'all', parseSearchPattern('hello'));
    expect(f.storedBytes).toBe(9);
    f.dispose();
  });

  it('跨分片边界: 模式尾部在前一片, 命中后一片', () => {
    const src = new RawDataBuffer(1024);
    push(src, 'rx', [0x00, 0x41, 0x42]); // ..AB
    push(src, 'rx', [0x43, 0x44, 0x00]); // CD..
    ticker.flush();

    // "ABCD" 跨边界: 第一片不含完整模式, 第二片 (拼接尾部 AB) 命中
    const f = new FilteredRawDataBuffer(src, 'all', parseSearchPattern('41424344'));
    expect(f.storedBytes).toBe(3);
    expect(Array.from(f.getLine(0).bytes)).toEqual([0x43, 0x44, 0x00]);
    f.dispose();
  });

  it('方向与搜索组合', () => {
    const src = new RawDataBuffer(1024);
    push(src, 'rx', [0xde, 0xad]);
    push(src, 'tx', [0xde, 0xad]);
    ticker.flush();

    const f = new FilteredRawDataBuffer(src, 'tx', parseSearchPattern('dead'));
    expect(f.storedBytes).toBe(2);
    expect(f.getLine(0).direction).toBe('tx');
    f.dispose();
  });
});

describe('FilteredRawDataBuffer 换行模式', () => {
  it('按 0x0A 分行, 行内容跨分片映射正确', () => {
    const src = new RawDataBuffer(1024);
    push(src, 'rx', [0x61, 0x62, 0x0a]); // "ab\n"
    push(src, 'rx', [0x63, 0x64, 0x0a, 0x65, 0x66]); // "cd\n" + "ef"
    ticker.flush();

    const f = new FilteredRawDataBuffer(src, 'all', null);
    expect(f.newlineLineCount).toBe(3);
    expect(Array.from(f.getNewlineLine(0).bytes)).toEqual([0x61, 0x62, 0x0a]);
    expect(Array.from(f.getNewlineLine(1).bytes)).toEqual([0x63, 0x64, 0x0a]);
    expect(Array.from(f.getNewlineLine(2).bytes)).toEqual([0x65, 0x66]);
    f.dispose();
  });
});

describe('FilteredRawDataBuffer 环形覆盖与清空', () => {
  it('源环覆盖后同步裁剪, 只保留窗口内数据', () => {
    const src = new RawDataBuffer(32);
    push(src, 'rx', bytes(16, 0));
    push(src, 'rx', bytes(16, 16));
    ticker.flush();

    const f = new FilteredRawDataBuffer(src, 'all', null);
    expect(f.storedBytes).toBe(32);

    // 再推 32 字节, 前两片被覆盖
    push(src, 'rx', bytes(16, 32));
    push(src, 'rx', bytes(16, 48));
    ticker.flush();

    expect(f.storedBytes).toBe(32);
    expect(f.lineCount).toBe(2);
    expect(Array.from(f.getLine(0).bytes)).toEqual(bytes(16, 32));
    expect(Array.from(f.getLine(1).bytes)).toEqual(bytes(16, 48));
    expect(f.droppedBytes).toBe(32);
    f.dispose();
  });

  it('clear 只清过滤视图, 源数据不受影响', () => {
    const src = new RawDataBuffer(1024);
    push(src, 'rx', bytes(16, 0));
    ticker.flush();

    const f = new FilteredRawDataBuffer(src, 'all', null);
    expect(f.lineCount).toBe(1);
    f.clear();
    expect(f.lineCount).toBe(0);
    expect(src.lineCount).toBe(1);

    push(src, 'rx', bytes(16, 16));
    ticker.flush();
    expect(f.lineCount).toBe(1);
    expect(Array.from(f.getLine(0).bytes)).toEqual(bytes(16, 16));
    f.dispose();
  });
});
