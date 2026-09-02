import { describe, it, expect } from 'vitest';
import { decodeEnvelopeFrame } from '../envelopeProtocol';

/// 按后端 encode_envelope 的布局手工构造 VENV 帧
function buildEnvelopeFrame(options: {
  seq: number;
  n: number;
  columns: number;
  firstTsMs: number;
  lastTsMs: number;
  bufferPoints: number;
  bufferCapacity: number;
  channels: { min: number[]; max: number[]; count: number[] }[];
}): ArrayBuffer {
  const { seq, n, columns, firstTsMs, lastTsMs, bufferPoints, bufferCapacity, channels } =
    options;
  const headerLen = 60;
  const payloadLen = channels.length * columns * 12;
  const buf = new ArrayBuffer(headerLen + payloadLen);
  const view = new DataView(buf);
  const encoder = new TextEncoder();
  new Uint8Array(buf).set(encoder.encode('VENV'), 0);
  view.setUint16(4, 1, true); // schema
  view.setUint16(6, 2, true); // event kind
  view.setBigUint64(8, BigInt(seq), true);
  view.setUint32(16, n, true);
  view.setUint32(20, columns, true);
  view.setUint32(24, channels.length, true);
  view.setBigInt64(28, BigInt(firstTsMs), true);
  view.setBigInt64(36, BigInt(lastTsMs), true);
  view.setUint32(44, bufferPoints, true);
  view.setUint32(48, bufferCapacity, true);
  view.setUint32(52, payloadLen, true);
  view.setUint32(56, headerLen, true);
  let offset = headerLen;
  for (const ch of channels) {
    for (const v of ch.min) {
      view.setFloat32(offset, v, true);
      offset += 4;
    }
    for (const v of ch.max) {
      view.setFloat32(offset, v, true);
      offset += 4;
    }
    for (const c of ch.count) {
      view.setUint32(offset, c, true);
      offset += 4;
    }
  }
  return buf;
}

describe('decodeEnvelopeFrame', () => {
  it('解码双通道帧: 数值/时间戳/空列标记', () => {
    const frame = decodeEnvelopeFrame(
      buildEnvelopeFrame({
        seq: 42,
        n: 100_000,
        columns: 4,
        firstTsMs: -1000,
        lastTsMs: 0,
        bufferPoints: 100_000,
        bufferCapacity: 100_000,
        channels: [
          {
            min: [-2.5, -1, Number.POSITIVE_INFINITY, -7],
            max: [2.5, 1, Number.NEGATIVE_INFINITY, 7],
            count: [100, 99, 0, 1],
          },
          {
            min: [0, -0, Number.POSITIVE_INFINITY, Number.POSITIVE_INFINITY],
            max: [0, 0, Number.NEGATIVE_INFINITY, Number.NEGATIVE_INFINITY],
            count: [50, 50, 0, 0],
          },
        ],
      }),
    );
    expect(frame.seq).toBe(42);
    expect(frame.n).toBe(100_000);
    expect(frame.columns).toBe(4);
    expect(frame.channelCount).toBe(2);
    expect(frame.firstTsMs).toBe(-1000);
    expect(frame.lastTsMs).toBe(0);
    expect(frame.bufferPoints).toBe(100_000);
    expect(frame.channels).toHaveLength(2);
    expect(Array.from(frame.channels[0].min)).toEqual([
      -2.5, -1, Number.POSITIVE_INFINITY, -7,
    ]);
    expect(Array.from(frame.channels[0].max)).toEqual([
      2.5, 1, Number.NEGATIVE_INFINITY, 7,
    ]);
    expect(Array.from(frame.channels[0].count)).toEqual([100, 99, 0, 1]);
    // -0 往返保号 (后端已归一为 +0, 解码侧不强求)
    expect(frame.channels[1].count).toEqual(new Uint32Array([50, 50, 0, 0]));
  });

  it('拒绝错误 magic / 截断帧 / 长度不匹配', () => {
    const good = buildEnvelopeFrame({
      seq: 1,
      n: 4,
      columns: 2,
      firstTsMs: 0,
      lastTsMs: 3,
      bufferPoints: 4,
      bufferCapacity: 10,
      channels: [{ min: [0, 0], max: [1, 1], count: [2, 2] }],
    });
    expect(() => decodeEnvelopeFrame(good)).not.toThrow();

    const badMagic = good.slice(0);
    new DataView(badMagic).setUint32(0, 0x12345678, true);
    expect(() => decodeEnvelopeFrame(badMagic)).toThrow(/magic/);

    expect(() => decodeEnvelopeFrame(good.slice(0, 30))).toThrow(/truncated/);

    const badLen = buildEnvelopeFrame({
      seq: 1,
      n: 4,
      columns: 2,
      firstTsMs: 0,
      lastTsMs: 3,
      bufferPoints: 4,
      bufferCapacity: 10,
      channels: [{ min: [0, 0], max: [1, 1], count: [2, 2] }],
    });
    // 声明 1 通道但 payload 只有 1 列 → 截断
    new DataView(badLen).setUint32(24, 2, true);
    expect(() => decodeEnvelopeFrame(badLen)).toThrow(/length mismatch/);
  });
});
