import { describe, expect, it } from 'vitest';
import { decodeWaveformWindow } from '../waveformProtocol';

/** 按 WWB1 布局构造 envelope (与 src-tauri waveform_binary.rs 契约一致) */
function buildEnvelope(): ArrayBuffer {
  const n = 3;
  const slots = 2;
  const derivedEntries = [
    { sink: 'w', source: 'm', handle: 'o', values: [1, NaN, 3] },
  ];
  const keysBytes = derivedEntries.reduce(
    (sum, e) => sum + e.sink.length + e.source.length + e.handle.length,
    0,
  );
  const pad = (4 - ((6 + keysBytes) % 4)) % 4;
  const total =
    64 +
    8 * n +
    4 * slots * n +
    derivedEntries.length * (6 + 4 * n) +
    keysBytes +
    pad;
  const buffer = new ArrayBuffer(total);
  const view = new DataView(buffer);
  const u8 = new Uint8Array(buffer);
  const enc = new TextEncoder();
  view.setUint32(0, 0x31425757, true); // "WWB1"
  view.setUint16(4, 1, true); // schema
  view.setUint16(6, 1, true); // sampling = min_max
  view.setBigUint64(8, 42n, true); // seq
  view.setBigUint64(16, 123_4567n, true); // latest_timestamp_us
  view.setBigUint64(24, 1000n, true); // buffer_points
  view.setBigUint64(32, 100_000n, true); // buffer_capacity
  view.setBigUint64(40, 2000n, true); // raw_window_points
  view.setUint32(48, n, true);
  view.setUint32(52, slots, true);
  view.setUint32(56, 2, true); // channel_count
  view.setUint32(60, derivedEntries.length, true);

  let cursor = 64;
  for (const ts of [-30, -20, -10]) {
    view.setFloat64(cursor, ts, true);
    cursor += 8;
  }
  for (const column of [[10, 11, 12], [NaN, 15, 16]]) {
    for (const v of column) {
      view.setFloat32(cursor, v, true);
      cursor += 4;
    }
  }
  for (const entry of derivedEntries) {
    for (const key of [entry.sink, entry.source, entry.handle]) {
      view.setUint16(cursor, key.length, true);
      u8.set(enc.encode(key), cursor + 2);
      cursor += 2 + key.length;
    }
    cursor += pad;
    for (const v of entry.values) {
      view.setFloat32(cursor, v, true);
      cursor += 4;
    }
  }
  return buffer;
}

describe('WWB1 waveform protocol', () => {
  it('decodes header, columns and derived series as zero-copy views', () => {
    const buffer = buildEnvelope();
    const win = decodeWaveformWindow(buffer);

    expect(win.seq).toBe(42);
    expect(win.sampling).toBe('min_max');
    expect(win.latest_timestamp_us).toBe(1_234_567);
    expect(win.buffer_points).toBe(1000);
    expect(win.buffer_capacity).toBe(100_000);
    expect(win.raw_window_points).toBe(2000);
    expect(win.channel_count).toBe(2);
    expect(win.timestamps).toBeInstanceOf(Float64Array);
    expect([...win.timestamps]).toEqual([-30, -20, -10]);
    expect(win.channels).toHaveLength(2);
    expect([...win.channels[0]]).toEqual([10, 11, 12]);
    expect(win.channels[1][0]).toBeNaN();
    expect([...win.channels[1].subarray(1)]).toEqual([15, 16]);

    const handle = win.derived.w.m.o;
    expect(handle).toBeInstanceOf(Float32Array);
    expect([...handle.subarray(0, 1)]).toEqual([1]);
    expect(handle[1]).toBeNaN();
    expect([...handle.subarray(2, 3)]).toEqual([3]);
  });

  it('rejects truncated and wrong-magic envelopes', () => {
    expect(() => decodeWaveformWindow(new ArrayBuffer(10))).toThrow();
    const buffer = buildEnvelope();
    new Uint8Array(buffer)[0] = 0x00;
    expect(() => decodeWaveformWindow(buffer)).toThrow();
  });
});
