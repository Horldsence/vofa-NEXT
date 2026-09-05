// ============ RawData 缓冲订阅 Hook ============
//
// RawDataView 的数据面: 节点旁路 (FrameDecoder raw) 与 Transport 字节源两条
// 引用计数订阅链 + 空缓冲占位 + 版本号推送。从视图组件拆出, 保持单文件 <500 行。

import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { getPortSampleStore } from '../../../lib/data/dataClient';
import { RawDataBuffer } from '../../../lib/buffers/dataBuffer';
import { acquireRawDataNode, releaseRawDataNode } from '../../../lib/buffers/rawDataNodeBuffer';
import { acquireRawDataTransport, releaseRawDataTransport } from '../../../lib/buffers/rawDataTransportBuffer';
import { useAppStore } from '../../../store/appStore';
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

// ---- 字符串通道 ----

export interface RawDataStringRow {
  /// 前端接收时刻 (字符串平面为 latest-value 快照, 无逐次时间戳)
  ts: number;
  text: string;
}

/// 字符串历史行数上限 (环形, 淘汰最旧)
const RAW_STRING_HISTORY_CAP = 1000;

/// 字符串通道历史 — 订阅 store 字符串平面 (全局单订阅, rAF 合并, 值变化才推送),
/// 按端口值变化累积历史行: latest-value 平面若逐 tick 记录会 30fps 刷屏,
/// 故日志语义 = 值变化才记录。通道切换时重置历史与去重基线。
export function useRawStringSamples(channel?: RawDataChannelRef) {
  const sourceId = channel?.sourceId;
  const port = channel?.sourceHandle ?? 'data';
  const textMap = useAppStore((s) => s.customTextOutputs);
  const [rows, setRows] = useState<RawDataStringRow[]>([]);
  // 去重基线: seen=false 表示尚无基线 (首次出现/清空后) — 下一次快照无条件入列
  const baselineRef = useRef<{ text: string; seen: boolean }>({ text: '', seen: false });

  useEffect(() => {
    baselineRef.current = { text: '', seen: false };
    setRows([]);
  }, [sourceId, port]);

  useEffect(() => {
    if (!sourceId) return;
    const text = textMap[sourceId]?.[port];
    if (text === undefined) return;
    const baseline = baselineRef.current;
    if (baseline.seen && baseline.text === text) return;
    baselineRef.current = { text, seen: true };
    const row: RawDataStringRow = { ts: Date.now(), text };
    setRows((prev) =>
      prev.length >= RAW_STRING_HISTORY_CAP
        ? [...prev.slice(prev.length - RAW_STRING_HISTORY_CAP + 1), row]
        : [...prev, row]
    );
  }, [textMap, sourceId, port]);

  const clear = useCallback(() => {
    // 当前值记为未入列基线: 下一次快照推送 (任意字符串输出变化) 时重新入列,
    // 视图不会因清空而永久错过当前值
    baselineRef.current = {
      text: (sourceId ? textMap[sourceId]?.[port] : undefined) ?? '',
      seen: false,
    };
    setRows([]);
  }, [textMap, sourceId, port]);

  return { rows, clear };
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
