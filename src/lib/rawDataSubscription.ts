import { invoke, Channel } from '@tauri-apps/api/core';
import type { RawDataBatch } from '../types';
import { closeTauriChannel } from './tauri';

/// 订阅原始数据 (后端周期性推送 raw_data_collector 中的最近分片)
/// 返回取消订阅函数
export function subscribeRawData(
  onEvent: (batch: RawDataBatch) => void,
  options?: { intervalMs?: number; maxBytes?: number }
): { cancel: () => void } {
  const channel = new Channel<RawDataBatch>();
  channel.onmessage = onEvent;
  void invoke('subscribe_rawdata', {
    onEvent: channel,
    intervalMs: options?.intervalMs,
    maxBytes: options?.maxBytes,
  });
  return {
    cancel: () => {
      void closeTauriChannel(channel, 'unsubscribe_rawdata', channel.id);
    },
  };
}

/// 清空后端原始数据收集器
export function clearRawDataBuffer(): Promise<void> {
  return invoke('clear_raw_data_collector');
}
