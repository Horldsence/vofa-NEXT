/**
 * 波形窗口 WWB1 二进制解码 — 波形 detail/overview 流的唯一 IPC 载荷。
 *
 * 后端为列式编码 (见 src-tauri/crates/cmd_display/src/waveform_binary.rs):
 * 时间戳 f64 + 每通道 f32 列。解码用 TypedArray 视图零拷贝读取,
 * NaN 原生传递 (JSON 时代 serde 会把 NaN 编码为 null, 还需一次归一化)。
 */

export interface DecodedWaveformWindow {
  seq: number;
  timestamps: Float64Array;
  channels: Float32Array[];
  channel_count: number;
  derived: Record<string, Record<string, Record<string, Float32Array>>>;
  buffer_points: number;
  buffer_capacity: number;
  latest_timestamp_us: number;
  raw_window_points: number;
  sampling: 'raw' | 'min_max' | 'lttb';
}

const HEADER_LEN = 64;
const MAGIC = 0x31425757; // "WWB1" little-endian
const SAMPLINGS = ['raw', 'min_max', 'lttb'] as const;

/** 解码 WWB1 波形窗口。所有数组均为底层 buffer 的视图 (零拷贝)。 */
export function decodeWaveformWindow(buffer: ArrayBuffer): DecodedWaveformWindow {
  if (buffer.byteLength < HEADER_LEN) {
    throw new Error('WWB1 waveform envelope is truncated');
  }
  const view = new DataView(buffer);
  if (view.getUint32(0, true) !== MAGIC) {
    throw new Error('WWB1 waveform envelope has invalid magic');
  }
  if (view.getUint16(4, true) !== 1) {
    throw new Error('Unsupported WWB1 schema version');
  }
  const sampling = SAMPLINGS[view.getUint16(6, true)] ?? 'raw';
  const seq = Number(view.getBigUint64(8, true));
  const latestTimestampUs = Number(view.getBigUint64(16, true));
  const bufferPoints = Number(view.getBigUint64(24, true));
  const bufferCapacity = Number(view.getBigUint64(32, true));
  const rawWindowPoints = Number(view.getBigUint64(40, true));
  const pointCount = view.getUint32(48, true);
  const slotCount = view.getUint32(52, true);
  const channelCount = view.getUint32(56, true);
  const derivedCount = view.getUint32(60, true);

  const timestampsBytes = pointCount * 8;
  const channelsBytes = slotCount * pointCount * 4;
  const derivedBytes = derivedCount * (6 + pointCount * 4);
  const expected = HEADER_LEN + timestampsBytes + channelsBytes + derivedBytes;
  if (buffer.byteLength < expected) {
    throw new Error('WWB1 waveform envelope has invalid lengths');
  }

  // 64 字节头 8 字节对齐, 时间戳列可直接视图读取
  const timestamps = new Float64Array(buffer, HEADER_LEN, pointCount);
  const channels: Float32Array[] = [];
  for (let slot = 0; slot < slotCount; slot++) {
    const offset = HEADER_LEN + timestampsBytes + slot * pointCount * 4;
    channels.push(new Float32Array(buffer, offset, pointCount));
  }

  let cursor = HEADER_LEN + timestampsBytes + channelsBytes;
  const derived: DecodedWaveformWindow['derived'] = {};
  for (let entry = 0; entry < derivedCount; entry++) {
    const readKey = () => {
      const len = view.getUint16(cursor, true);
      const key = new TextDecoder().decode(
        new Uint8Array(buffer, cursor + 2, len),
      );
      cursor += 2 + len;
      return key;
    };
    const sink = readKey();
    const source = readKey();
    const handle = readKey();
    // 后端在键名后补齐到 4 字节边界 (f32 列视图的对齐要求)
    cursor = (cursor + 3) & ~3;
    const values = new Float32Array(buffer, cursor, pointCount);
    cursor += pointCount * 4;
    const handles = (derived[sink] ??= {})[source] ??= {};
    handles[handle] = values;
  }

  return {
    seq,
    timestamps,
    channels,
    channel_count: channelCount,
    derived,
    buffer_points: bufferPoints,
    buffer_capacity: bufferCapacity,
    latest_timestamp_us: latestTimestampUs,
    raw_window_points: rawWindowPoints,
    sampling,
  };
}
