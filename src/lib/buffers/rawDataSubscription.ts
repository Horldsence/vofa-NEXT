import { invoke } from '@tauri-apps/api/core';
import type { RawDataBatch, RawDataDirection } from '../../types';
import { subscribeDisplay } from './shardedSubscription';
import { tickMetric } from '../utils/perfLog';

export type DirectionFilter = 'all' | RawDataDirection;

export interface RawDataFilterOptions {
  directionFilter: DirectionFilter;
  searchTerm: string;
}

/// 统计 base64 载荷字节速率 (解码前)
function countBytes(key: string, onEvent: (batch: RawDataBatch) => void) {
  return (batch: RawDataBatch) => {
    let bytes = 0;
    for (const c of batch.chunks) bytes += c.bytes_b64.length;
    tickMetric(key, bytes);
    onEvent(batch);
  };
}

/// 订阅原始数据。每个逻辑源使用一个后端有序流和一个 IPC Channel。
export function subscribeRawData(
  source: string,
  onEvent: (batch: RawDataBatch) => void,
  options?: { intervalMs?: number; maxBytes?: number },
): { cancel: () => void } {
  return subscribeDisplay<RawDataBatch>(
    {
      kind: 'raw_data',
      origin: { kind: 'transport', id: source },
      direction: '',
      search: '',
    },
    'raw_data',
    countBytes('rawdata:global', onEvent),
    { intervalMs: options?.intervalMs, maxItems: options?.maxBytes },
  );
}

/// 订阅指定节点的原始数据 — 统一分片流 (该节点输出字节流)
/// 返回取消订阅函数
export function subscribeRawDataNode(
  nodeId: string,
  onEvent: (batch: RawDataBatch) => void,
  options?: { intervalMs?: number; maxBytes?: number },
): { cancel: () => void } {
  return subscribeDisplay<RawDataBatch>(
    {
      kind: 'raw_data',
      origin: { kind: 'decoder', id: nodeId },
      direction: '',
      search: '',
    },
    'raw_data',
    onEvent,
    { intervalMs: options?.intervalMs, maxItems: options?.maxBytes },
  );
}

/// 订阅带方向与搜索过滤的原始数据 — 统一分片流
///
/// 后端只推送方向匹配且包含搜索模式的 chunk, 前端无需再遍历过滤。
export function subscribeRawDataFiltered(
  source: string,
  filter: RawDataFilterOptions,
  onEvent: (batch: RawDataBatch) => void,
  options?: { intervalMs?: number; maxBytes?: number },
): { cancel: () => void } {
  return subscribeDisplay<RawDataBatch>(
    {
      kind: 'raw_data',
      origin: { kind: 'transport', id: source },
      direction: filter.directionFilter,
      search: filter.searchTerm,
    },
    'raw_data',
    countBytes('rawdata:filtered', onEvent),
    { intervalMs: options?.intervalMs, maxItems: options?.maxBytes },
  );
}

/// 订阅带方向与搜索过滤的节点原始数据 — 统一分片流
export function subscribeRawDataNodeFiltered(
  nodeId: string,
  filter: RawDataFilterOptions,
  onEvent: (batch: RawDataBatch) => void,
  options?: { intervalMs?: number; maxBytes?: number },
): { cancel: () => void } {
  return subscribeDisplay<RawDataBatch>(
    {
      kind: 'raw_data',
      origin: { kind: 'decoder', id: nodeId },
      direction: filter.directionFilter,
      search: filter.searchTerm,
    },
    'raw_data',
    onEvent,
    { intervalMs: options?.intervalMs, maxItems: options?.maxBytes },
  );
}

/// 清空后端原始数据收集器 (source = Transport 节点 id; 缺省清空全部源)
export function clearRawDataBuffer(source?: string): Promise<void> {
  return invoke('clear_raw_data_collector', { source: source ?? null });
}
