import { describe, expect, it } from 'vitest';
import { decodeSampleEnvelope } from '../sampleProtocol';

function envelope(value: number, status = 1): ArrayBuffer {
  const buffer = new ArrayBuffer(85);
  const view = new DataView(buffer);
  view.setUint32(0, 0x50444e56, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, 1, true);
  view.setUint16(8, status, true);
  view.setBigUint64(12, 7n, true);
  view.setBigUint64(20, 9n, true);
  view.setUint32(28, 1, true);
  view.setUint32(60, 17, true);
  view.setUint32(64, 68, true);
  view.setBigUint64(68, 11_000n, true);
  view.setFloat64(76, value, true);
  view.setUint8(84, 1);
  return buffer;
}

describe('VNDP sample protocol', () => {
  it('preserves a real zero sample', () => {
    const batch = decodeSampleEnvelope(envelope(0));
    expect(batch.status).toBe('live');
    expect(batch.rows).toEqual([{ seq: 9, ts: 11, value: 0 }]);
  });

  it('represents missing data as status without a fabricated row', () => {
    const buffer = envelope(0, 3).slice(0, 68);
    const view = new DataView(buffer);
    view.setUint32(28, 0, true);
    view.setUint32(60, 0, true);
    const batch = decodeSampleEnvelope(buffer);
    expect(batch.status).toBe('channel_out_of_range');
    expect(batch.rows).toEqual([]);
  });

  it('honours the validity bitmap instead of decoding an invalid value as zero', () => {
    const buffer = envelope(0);
    new DataView(buffer).setUint8(84, 0);
    expect(decodeSampleEnvelope(buffer).rows).toEqual([]);
  });
});
