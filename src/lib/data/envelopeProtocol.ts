/// VENV v1 波形包络帧解码 — 后端 cmd_display/src/stream.rs `encode_envelope` 的镜像。
///
/// 布局 (little-endian): 60 字节头 + 每通道 columns×(f32 min, f32 max, u32 count)。
/// 空列: min=+inf / max=-inf / count=0 — 调用方按断线处理 (count === 0)。
/// 视图零拷贝: min/max/count 直接建在原 buffer 上 (头长 60 为 4 字节对齐)。

const HEADER_LEN = 60;
const MAGIC = 0x564e4556; // bytes "VENV" read little-endian
const SCHEMA_VERSION = 1;
const EVENT_KIND = 2;

export interface WaveformEnvelopeChannel {
  /// 列内非 NaN 样本最小值 (空列 +inf)
  min: Float32Array;
  /// 列内非 NaN 样本最大值 (空列 -inf)
  max: Float32Array;
  /// 列内非 NaN 样本数 (0 = 空列/断线)
  count: Uint32Array;
}

export interface WaveformEnvelopeFrame {
  seq: number;
  /// 参与压缩的窗口点数
  n: number;
  columns: number;
  channelCount: number;
  /// 窗口首/尾样本时间戳 (毫秒)
  firstTsMs: number;
  lastTsMs: number;
  bufferPoints: number;
  bufferCapacity: number;
  channels: WaveformEnvelopeChannel[];
}

function safeNumber(value: bigint): number {
  return value > BigInt(Number.MAX_SAFE_INTEGER)
    ? Number.MAX_SAFE_INTEGER
    : Number(value);
}

export function decodeEnvelopeFrame(buffer: ArrayBuffer): WaveformEnvelopeFrame {
  if (buffer.byteLength < HEADER_LEN)
    throw new Error('VENV envelope is truncated');
  const view = new DataView(buffer);
  if (view.getUint32(0, true) !== MAGIC)
    throw new Error('VENV envelope has invalid magic');
  if (view.getUint16(4, true) !== SCHEMA_VERSION)
    throw new Error('Unsupported VENV schema version');
  if (view.getUint16(6, true) !== EVENT_KIND)
    throw new Error('Unsupported VENV event kind');

  const seq = safeNumber(view.getBigUint64(8, true));
  const n = view.getUint32(16, true);
  const columns = view.getUint32(20, true);
  const channelCount = view.getUint32(24, true);
  const firstTsMs = safeNumber(view.getBigInt64(28, true));
  const lastTsMs = safeNumber(view.getBigInt64(36, true));
  const bufferPoints = view.getUint32(44, true);
  const bufferCapacity = view.getUint32(48, true);
  const payloadLen = view.getUint32(52, true);
  const headerLen = view.getUint32(56, true);

  const expected = channelCount * columns * 12;
  if (headerLen !== HEADER_LEN)
    throw new Error(`Unexpected VENV header length ${headerLen}`);
  if (buffer.byteLength - HEADER_LEN < expected || payloadLen !== expected)
    throw new Error('VENV envelope payload length mismatch');

  const channels: WaveformEnvelopeChannel[] = [];
  let offset = HEADER_LEN;
  for (let c = 0; c < channelCount; c++) {
    const min = new Float32Array(buffer, offset, columns);
    offset += columns * 4;
    const max = new Float32Array(buffer, offset, columns);
    offset += columns * 4;
    const count = new Uint32Array(buffer, offset, columns);
    offset += columns * 4;
    channels.push({ min, max, count });
  }

  return {
    seq,
    n,
    columns,
    channelCount,
    firstTsMs,
    lastTsMs,
    bufferPoints,
    bufferCapacity,
    channels,
  };
}
