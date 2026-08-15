import { invoke } from '@tauri-apps/api/core';
import type { RawDataBatch } from '../../types';
import { makeOrderedSink, subscribeSharded } from './shardedSubscription';

/// 订阅原始数据 — 统一分片流 (增量 drain + 自动并发分片)
///
/// 单 channel 够用时只有 shard 0 工作 (等价于旧行为), 积压超过阈值时
/// 后端自动多通道并行推送; 每批带组级 seq, 在此重组后交付。
/// 返回取消订阅函数 (取消全部分片)
export function subscribeRawData(
  onEvent: (batch: RawDataBatch) => void,
  options?: { intervalMs?: number; maxBytes?: number }
): { cancel: () => void } {
  return subscribeSharded<RawDataBatch>(
    'subscribe_rawdata',
    'unsubscribe_rawdata',
    {},
    makeOrderedSink(onEvent),
    { intervalMs: options?.intervalMs, maxBytes: options?.maxBytes }
  );
}

/// 订阅指定节点的原始数据 — 统一分片流 (该节点输出字节流)
/// 返回取消订阅函数
export function subscribeRawDataNode(
  nodeId: string,
  onEvent: (batch: RawDataBatch) => void,
  options?: { intervalMs?: number; maxBytes?: number }
): { cancel: () => void } {
  return subscribeSharded<RawDataBatch>(
    'subscribe_rawdata_node',
    'unsubscribe_rawdata_node',
    { nodeId },
    makeOrderedSink(onEvent),
    { intervalMs: options?.intervalMs, maxBytes: options?.maxBytes }
  );
}

/// 清空后端原始数据收集器
export function clearRawDataBuffer(): Promise<void> {
  return invoke('clear_raw_data_collector');
}
