import { RawDataBuffer } from './dataBuffer';
import { subscribeRawData } from './rawDataSubscription';

interface RawDataTransportEntry {
  buffer: RawDataBuffer;
  refs: number;
  cancel: (() => void) | null;
}

/// 按 Transport 节点注册的原始数据 buffer 注册表 (引用计数)
/// RawData 控件的字节源通道 (Transport rx 直连 / Protocol out 上溯) 查看
/// 非全局选中接口时共享一条后端订阅, 最后一个引用释放时取消订阅并移除注册。
const registry = new Map<string, RawDataTransportEntry>();

/// 获取指定 Transport 节点的原始数据 buffer (引用 +1)
/// 不存在时创建新 buffer 并启动后端订阅 (rx/tx 均入该收集器)
export function acquireRawDataTransport(transportId: string): RawDataBuffer {
  const existing = registry.get(transportId);
  if (existing) {
    existing.refs++;
    return existing.buffer;
  }
  const buffer = new RawDataBuffer();
  const { cancel } = subscribeRawData(
    transportId,
    (batch) => buffer.pushBatch(batch),
    { intervalMs: 100, maxBytes: 65536 }
  );
  registry.set(transportId, { buffer, refs: 1, cancel });
  return buffer;
}

/// 释放指定 Transport 节点的原始数据 buffer (引用 -1)
/// 引用归零时取消后端订阅并从注册表移除
export function releaseRawDataTransport(transportId: string): void {
  const entry = registry.get(transportId);
  if (!entry) return;
  entry.refs--;
  if (entry.refs <= 0) {
    entry.cancel?.();
    registry.delete(transportId);
  }
}
