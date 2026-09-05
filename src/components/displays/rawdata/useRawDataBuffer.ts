// ============ RawData 缓冲订阅 Hook ============
//
// RawDataView 的数据面: 节点旁路 (FrameDecoder raw) 与 Transport 字节源两条
// 引用计数订阅链 + 空缓冲占位 + 版本号推送。从视图组件拆出, 保持单文件 <500 行。

import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { getPortSampleStore } from '../../../lib/data/dataClient';
import { RawDataBuffer } from '../../../lib/buffers/dataBuffer';
import { acquireRawDataNode, releaseRawDataNode } from '../../../lib/buffers/rawDataNodeBuffer';
import { acquireRawDataTransport, releaseRawDataTransport } from '../../../lib/buffers/rawDataTransportBuffer';
import type { RawDataFilterOptions } from '../../../lib/buffers/rawDataSubscription';

interface RawDataChannelRef {
  sourceId: string;
  sourceHandle?: string;
}

/// 数值通道样本订阅 — 数值口视图直接消费端口快照
export function useRawNumericSamples(channel?: RawDataChannelRef) {
  const sampleStore = useMemo(
    () => getPortSampleStore(channel?.sourceId, channel?.sourceHandle ?? 'data'),
    [channel?.sourceId, channel?.sourceHandle]
  );
  const snapshot = useSyncExternalStore(sampleStore.subscribe, sampleStore.getSnapshot, sampleStore.getSnapshot);
  return { snapshot, clearSamples: sampleStore.clear };
}

interface UseRawDataBufferOptions {
  /// FrameDecoder raw 口的订阅 key (sourceId); null = 非节点旁路
  nodeBufferKey: string | null;
  /// 字节源通道上溯到的 Transport id; null = 无 transport (用空缓冲占位)
  byteTransportId: string | null;
  isByteSrc: boolean;
  backendFilter?: RawDataFilterOptions;
}

export function useRawDataBuffer({ nodeBufferKey, byteTransportId, isByteSrc, backendFilter }: UseRawDataBufferOptions): {
  buffer: RawDataBuffer;
  version: number;
} {
  // 节点 buffer。方向和搜索由后端订阅源执行。
  const [nodeBuffer, setNodeBuffer] = useState<RawDataBuffer | null>(null);
  useEffect(() => {
    if (!nodeBufferKey) {
      setNodeBuffer(null);
      return;
    }
    const acquired = acquireRawDataNode(nodeBufferKey, backendFilter);
    setNodeBuffer(acquired);
    return () => releaseRawDataNode(nodeBufferKey, backendFilter);
  }, [nodeBufferKey, backendFilter]);

  // 字节源通道 buffer: 按 Transport 引用计数获取 (同 Transport 多卡片自动共享同一订阅);
  // 上溯失败 (无 transportId) 用空 buffer 占位；RawData 不再维持隐藏的全局订阅。
  const transportBufferKey = byteTransportId ?? null;
  const [transportBuffer, setTransportBuffer] = useState<RawDataBuffer | null>(null);
  useEffect(() => {
    if (!transportBufferKey) {
      setTransportBuffer(null);
      return;
    }
    const acquired = acquireRawDataTransport(transportBufferKey, backendFilter);
    setTransportBuffer(acquired);
    return () => releaseRawDataTransport(transportBufferKey, backendFilter);
  }, [transportBufferKey, backendFilter]);

  const emptyByteBufferRef = useRef<RawDataBuffer | null>(null);
  // 惰性取空 buffer (占位: 无 transportId / 无连线时保持订阅链类型完整)
  const getEmptyByteBuffer = useCallback((): RawDataBuffer => {
    emptyByteBufferRef.current ??= new RawDataBuffer();
    return emptyByteBufferRef.current;
  }, []);
  const byteSourceBuffer = !isByteSrc
    ? null
    : byteTransportId
      ? transportBuffer
      : getEmptyByteBuffer();

  const buffer = nodeBuffer ?? byteSourceBuffer ?? getEmptyByteBuffer();

  // 强制重新渲染的版本号
  const [version, setVersion] = useState(0);
  useEffect(() => {
    return buffer.subscribe(() => setVersion((v) => v + 1));
  }, [buffer]);

  return { buffer, version };
}

/// 调试: 长任务监控 — 主线程单次任务 >100ms 即记录 (卡死定位)
export function useLongTaskMonitor(): void {
  useEffect(() => {
    if (typeof PerformanceObserver === 'undefined') return;
    try {
      const obs = new PerformanceObserver((list) => {
        if (list.getEntries().length > 0) {
          console.info('[raw-data] long task detected');
        }
      });
      obs.observe({ entryTypes: ['longtask'] });
      return () => obs.disconnect();
    } catch {
      return;
    }
  }, []);
}
