import type { RawDataBuffer } from './dataBuffer';
import {
  subscribeRawDataNode,
  subscribeRawDataNodeFiltered,
  type RawDataFilterOptions,
} from './rawDataSubscription';
import {
  createRawDataPreviewBuffer,
  trackRawDataPreviewBuffer,
} from './rawDataPreviewRegistry';

interface RawDataNodeEntry {
  buffer: RawDataBuffer;
  refs: number;
  cancel: (() => void) | null;
  untrack: () => void;
}

/// 按节点注册的原始数据 buffer 注册表 (引用计数)
/// 多个 RawData 控件查看同一个 FrameDecoder 节点时共享一条后端订阅,
/// 最后一个引用释放时取消订阅并移除注册。
const registry = new Map<string, RawDataNodeEntry>();

/// 获取指定节点的原始数据 buffer (引用 +1)
/// 不存在时创建新 buffer 并启动后端订阅
function registryKey(nodeId: string, filter?: RawDataFilterOptions): string {
  return `${nodeId}\u0000${filter?.directionFilter ?? 'all'}\u0000${filter?.searchTerm ?? ''}`;
}

export function acquireRawDataNode(
  nodeId: string,
  filter?: RawDataFilterOptions
): RawDataBuffer | null {
  const key = registryKey(nodeId, filter);
  const existing = registry.get(key);
  if (existing) {
    existing.refs++;
    return existing.buffer;
  }
  const buffer = createRawDataPreviewBuffer();
  const untrack = trackRawDataPreviewBuffer(buffer);
  const options = { intervalMs: 100, maxBytes: 65536 };
  const { cancel } = filter
    ? subscribeRawDataNodeFiltered(nodeId, filter, (batch) => buffer.pushBatch(batch), options)
    : subscribeRawDataNode(nodeId, (batch) => buffer.pushBatch(batch), options);
  registry.set(key, { buffer, refs: 1, cancel, untrack });
  return buffer;
}

/// 释放指定节点的原始数据 buffer (引用 -1)
/// 引用归零时取消后端订阅并从注册表移除
export function releaseRawDataNode(nodeId: string, filter?: RawDataFilterOptions): void {
  const key = registryKey(nodeId, filter);
  const entry = registry.get(key);
  if (!entry) return;
  entry.refs--;
  if (entry.refs <= 0) {
    entry.cancel?.();
    entry.untrack();
    registry.delete(key);
  }
}

/// 查询指定节点的原始数据 buffer (不改变引用计数)
export function getRawDataNodeBuffer(nodeId: string): RawDataBuffer | undefined {
  const prefix = `${nodeId}\u0000`;
  for (const [key, entry] of registry) {
    if (key.startsWith(prefix)) return entry.buffer;
  }
  return undefined;
}
