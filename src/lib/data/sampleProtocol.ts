export type PortSampleStatus =
  'waiting' | 'live' | 'disconnected' | 'channel_out_of_range' | 'overrun';

export interface DecodedSampleBatch {
  sequence: number;
  status: PortSampleStatus;
  rows: { seq: number; ts: number; value: number }[];
  previewSkipped: number;
  retentionEvicted: number;
  ingressDropped: number;
  byteLength: number;
}

const HEADER_LEN = 68;
const MAGIC = 0x50444e56; // bytes "VNDP" read little-endian

function safeNumber(value: bigint): number {
  return value > BigInt(Number.MAX_SAFE_INTEGER)
    ? Number.MAX_SAFE_INTEGER
    : Number(value);
}

export function decodeSampleEnvelope(buffer: ArrayBuffer): DecodedSampleBatch {
  if (buffer.byteLength < HEADER_LEN)
    throw new Error('VNDP sample envelope is truncated');
  const view = new DataView(buffer);
  if (view.getUint32(0, true) !== MAGIC)
    throw new Error('VNDP sample envelope has invalid magic');
  if (view.getUint16(4, true) !== 1)
    throw new Error('Unsupported VNDP schema version');
  if (view.getUint16(6, true) !== 1)
    throw new Error('Unsupported VNDP event kind');

  const statuses: PortSampleStatus[] = [
    'waiting',
    'live',
    'disconnected',
    'channel_out_of_range',
    'overrun',
  ];
  const status = statuses[view.getUint16(8, true)] ?? 'overrun';
  const sequence = safeNumber(view.getBigUint64(12, true));
  const firstSample = safeNumber(view.getBigUint64(20, true));
  const count = view.getUint32(28, true);
  const previewSkipped = safeNumber(view.getBigUint64(36, true));
  const retentionEvicted = safeNumber(view.getBigUint64(44, true));
  const ingressDropped = safeNumber(view.getBigUint64(52, true));
  const payloadLength = view.getUint32(60, true);
  const headerLength = view.getUint32(64, true);
  const validityLength = Math.ceil(count / 8);
  const expectedPayload = count * 16 + validityLength;
  if (
    headerLength !== HEADER_LEN ||
    payloadLength !== expectedPayload ||
    headerLength + payloadLength > buffer.byteLength
  ) {
    throw new Error('VNDP sample envelope has invalid lengths');
  }

  const rows: { seq: number; ts: number; value: number }[] = [];
  const valuesOffset = headerLength + count * 8;
  const validityOffset = valuesOffset + count * 8;
  for (let i = 0; i < count; i++) {
    const isValid =
      (view.getUint8(validityOffset + Math.floor(i / 8)) & (1 << (i % 8))) !==
      0;
    if (!isValid) continue;
    const timestampUs = safeNumber(
      view.getBigUint64(headerLength + i * 8, true),
    );
    rows.push({
      seq: firstSample + i,
      ts: timestampUs / 1000,
      value: view.getFloat64(valuesOffset + i * 8, true),
    });
  }
  return {
    sequence,
    status,
    rows,
    previewSkipped,
    retentionEvicted,
    ingressDropped,
    byteLength: buffer.byteLength,
  };
}
