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

/// 逻辑采样过滤条件 — 与 Rust LogicSampleFilter 对应 (全部缺省 = 匹配全部)
export interface LogicSampleFilterOptions {
  /// 通道位图掩码 — 只关心这些通道
  channel_mask?: number;
  /// 期望通道值 (与掩码组合, (channels & mask) == (value & mask))
  channel_value?: number;
}

/// 订阅带过滤条件的逻辑采样 — 统一分片流
///
/// 后端只推送匹配 filter 的采样; 游标从最旧可读位置开始, 先拉历史匹配, 之后增量。
export function subscribeLogicSamplesFiltered(
  filter: LogicSampleFilterOptions,
  onEvent: (batch: LogicSampleBatch) => void,
  options?: { intervalMs?: number; maxSamples?: number }
): { cancel: () => void } {
  return subscribeSharded<LogicSampleBatch>(
    'subscribe_logic_samples_filtered',
    'unsubscribe_logic_samples',
    { filter },
    makeOrderedSink(onEvent),
    { intervalMs: options?.intervalMs, maxSamples: options?.maxSamples }
  );
}

/// 解码事件过滤条件 — 与 Rust DecodedEventFilter 对应 (全部缺省 = 匹配全部)
export interface DecodedEventFilterOptions {
  /// 协议类型: "uart" | "i2c" | "spi"
  kind?: string;
  /// 载荷字节子串匹配
  byte_pattern?: number[];
}

/// 订阅带过滤条件的解码事件 — 统一分片流
///
/// 后端只推送匹配 filter 的事件; 游标从最旧可读位置开始, 先拉历史匹配, 之后增量。
export function subscribeDecodedEventsFiltered(
  filter: DecodedEventFilterOptions,
  onEvent: (batch: DecodedEventBatch) => void,
  options?: { intervalMs?: number; maxEvents?: number }
): { cancel: () => void } {
  return subscribeSharded<DecodedEventBatch>(
    'subscribe_decoded_events_filtered',
    'unsubscribe_decoded_events',
    { filter },
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
