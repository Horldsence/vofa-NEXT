import { RawDataBuffer } from './dataBuffer';
import {
  subscribeRawData,
  subscribeRawDataFiltered,
  type RawDataFilterOptions,
} from './rawDataSubscription';
import {
  createRawDataPreviewBuffer,
  trackRawDataPreviewBuffer,
} from './rawDataPreviewRegistry';

interface RawDataTransportEntry {
  buffer: RawDataBuffer;
  refs: number;
  cancel: (() => void) | null;
  untrack: () => void;
}

/// 按 Transport 节点注册的原始数据 buffer 注册表 (引用计数)
/// RawData 控件的字节源通道 (Transport rx 直连 / Protocol out 上溯) 查看
/// 非全局选中接口时共享一条后端订阅, 最后一个引用释放时取消订阅并移除注册。
const registry = new Map<string, RawDataTransportEntry>();

const SUBSCRIBE_OPTIONS = { intervalMs: 100, maxBytes: 65536 } as const;

/// 为 buffer 建立后端订阅组, 返回取消函数
function registryKey(transportId: string, filter?: RawDataFilterOptions): string {
  return `${transportId}\u0000${filter?.directionFilter ?? 'all'}\u0000${filter?.searchTerm ?? ''}`;
}

function subscribeBuffer(
  transportId: string,
  buffer: RawDataBuffer,
  filter?: RawDataFilterOptions
): () => void {
  const handler = (batch: import('../../types').RawDataBatch) => buffer.pushBatch(batch);
  const { cancel } = filter
    ? subscribeRawDataFiltered(transportId, filter, handler, SUBSCRIBE_OPTIONS)
    : subscribeRawData(transportId, handler, SUBSCRIBE_OPTIONS);
  return cancel;
}

/// 获取指定 Transport 节点的原始数据 buffer (引用 +1)
/// 不存在时创建新 buffer 并启动后端订阅 (rx/tx 均入该收集器)
export function acquireRawDataTransport(
  transportId: string,
  filter?: RawDataFilterOptions
): RawDataBuffer {
  const key = registryKey(transportId, filter);
  const existing = registry.get(key);
  if (existing) {
    existing.refs++;
    return existing.buffer;
  }
  const buffer = createRawDataPreviewBuffer();
  const untrack = trackRawDataPreviewBuffer(buffer);
  const cancel = subscribeBuffer(transportId, buffer, filter);
  registry.set(key, { buffer, refs: 1, cancel, untrack });
  return buffer;
}

/// 释放指定 Transport 节点的原始数据 buffer (引用 -1)
/// 引用归零时取消后端订阅并从注册表移除
export function releaseRawDataTransport(transportId: string, filter?: RawDataFilterOptions): void {
  const key = registryKey(transportId, filter);
  const entry = registry.get(key);
  if (!entry) return;
  entry.refs--;
  if (entry.refs <= 0) {
    entry.cancel?.();
    entry.untrack();
    registry.delete(key);
  }
}

export function clearRawDataTransportBuffers(transportId: string): void {
  const prefix = `${transportId}\u0000`;
  for (const [key, entry] of registry) {
    if (key.startsWith(prefix)) entry.buffer.clear();
  }
}
