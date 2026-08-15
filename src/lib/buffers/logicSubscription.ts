import { invoke } from '@tauri-apps/api/core';
import type { LogicSampleBatch, DecodedEventBatch } from '../../types';
import { makeOrderedSink, subscribeSharded } from './shardedSubscription';

/// 订阅逻辑采样数据 — 统一分片流 (增量 drain, 首批回溯最近历史, 之后严格增量无重复)
/// 返回取消订阅函数
export function subscribeLogicSamples(
  onEvent: (batch: LogicSampleBatch) => void,
  options?: { intervalMs?: number; maxSamples?: number }
): { cancel: () => void } {
  return subscribeSharded<LogicSampleBatch>(
    'subscribe_logic_samples',
    'unsubscribe_logic_samples',
    {},
    makeOrderedSink(onEvent),
    { intervalMs: options?.intervalMs, maxSamples: options?.maxSamples }
  );
}

/// 订阅解码事件 — 统一分片流 (增量 drain, 首批回溯最近历史, 之后严格增量无重复)
/// 返回取消订阅函数
export function subscribeDecodedEvents(
  onEvent: (batch: DecodedEventBatch) => void,
  options?: { intervalMs?: number; maxEvents?: number }
): { cancel: () => void } {
  return subscribeSharded<DecodedEventBatch>(
    'subscribe_decoded_events',
    'unsubscribe_decoded_events',
    {},
    makeOrderedSink(onEvent),
    { intervalMs: options?.intervalMs, maxEvents: options?.maxEvents }
  );
}

/// 同步查询: 获取最近 N 个逻辑采样
export function getRecentLogicSamples(count: number): Promise<LogicSampleBatch> {
  return invoke('get_recent_logic_samples', { count });
}

/// 清空逻辑采样缓冲区
export function clearLogicBuffer(): Promise<void> {
  return invoke('clear_logic_buffer');
}

/// 同步查询: 获取最近 N 个解码事件
export function getRecentDecodedEvents(count: number): Promise<DecodedEventBatch> {
  return invoke('get_recent_decoded_events', { count });
}

/// 清空解码事件缓冲区
export function clearDecodedBuffer(): Promise<void> {
  return invoke('clear_decoded_buffer');
}
