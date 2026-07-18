import { invoke, Channel } from '@tauri-apps/api/core';
import type { LogicSampleBatch, DecodedEventBatch } from '../types';
import { closeTauriChannel } from './tauri';

/// 订阅逻辑采样数据 (后端周期性推送 logic_buffer 中的最近采样)
/// 返回取消订阅函数
export function subscribeLogicSamples(
  onEvent: (batch: LogicSampleBatch) => void,
  options?: { intervalMs?: number; maxSamples?: number }
): { cancel: () => void } {
  const channel = new Channel<LogicSampleBatch>();
  channel.onmessage = onEvent;
  void invoke('subscribe_logic_samples', {
    onEvent: channel,
    intervalMs: options?.intervalMs,
    maxSamples: options?.maxSamples,
  });
  return {
    cancel: () => {
      void closeTauriChannel(channel, 'unsubscribe_logic_samples', channel.id);
    },
  };
}

/// 订阅解码事件 (后端周期性推送 decoded_buffer 中的最近事件)
/// 返回取消订阅函数
export function subscribeDecodedEvents(
  onEvent: (batch: DecodedEventBatch) => void,
  options?: { intervalMs?: number; maxEvents?: number }
): { cancel: () => void } {
  const channel = new Channel<DecodedEventBatch>();
  channel.onmessage = onEvent;
  void invoke('subscribe_decoded_events', {
    onEvent: channel,
    intervalMs: options?.intervalMs,
    maxEvents: options?.maxEvents,
  });
  return {
    cancel: () => {
      void closeTauriChannel(channel, 'unsubscribe_decoded_events', channel.id);
    },
  };
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
