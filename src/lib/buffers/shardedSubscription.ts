import { invoke, Channel } from '@tauri-apps/api/core';
import { closeTauriChannel } from '../tauri/tauri';
import { tickMetric, perfEvent } from '../utils/perfLog';

export type DisplayKind =
  | 'graph_outputs'
  | 'custom_inputs'
  | 'string_outputs'
  | 'spectrum'
  | 'waveform'
  | 'raw_data'
  | 'can_frames'
  | 'logic_samples'
  | 'decoded_events'
  | 'can_load';

export interface DisplayEvent<T> {
  kind: DisplayKind;
  payload: T;
}

export function subscribeDisplaySnapshot<T>(
  request: Record<string, unknown>,
  expectedKind: DisplayKind,
  sink: (value: T) => void,
  intervalMs?: number,
): { cancel: () => void } {
  const channel = new Channel<DisplayEvent<T>>();
  channel.onmessage = (event) => {
    if (event.kind === expectedKind) sink(event.payload);
  };
  void invoke('subscribe_data', {
    request,
    onEvent: channel,
    intervalMs,
    maxItems: null,
  });
  return {
    cancel: () =>
      void closeTauriChannel(channel, 'unsubscribe_data', channel.id),
  };
}

/// 统一单通道订阅。后端 Actor 保证事件顺序，前端不再创建分片或重排批次。
export function subscribeDisplay<T>(
  request: Record<string, unknown>,
  expectedKind: DisplayKind,
  sink: (batch: T) => void,
  options?: { intervalMs?: number; maxItems?: number },
): { cancel: () => void } {
  const channels: Channel<DisplayEvent<T>>[] = [];
  let cancelled = false;

  // 调试: 统计该订阅的消息速率 (payload 字节由各订阅包装补充)
  const countedSink = (event: DisplayEvent<T>) => {
    if (event.kind !== expectedKind) {
      console.error(
        `显示订阅类型不匹配: expected=${expectedKind}, actual=${event.kind}`,
      );
      return;
    }
    tickMetric(`display:${expectedKind}`);
    sink(event.payload);
  };
  perfEvent(`subscribe display:${expectedKind}`);

  void (async () => {
    try {
      const first = new Channel<DisplayEvent<T>>();
      first.onmessage = countedSink;
      channels.push(first);
      await invoke('subscribe_data', {
        request,
        onEvent: first,
        ...options,
      });
      if (cancelled) {
        void closeTauriChannel(first, 'unsubscribe_data', first.id);
      }
    } catch (e) {
      console.error(`显示订阅失败 (${expectedKind}):`, e);
    }
  })();

  return {
    cancel: () => {
      cancelled = true;
      perfEvent(`cancel display:${expectedKind} (${channels.length} channels)`);
      for (const ch of channels)
        void closeTauriChannel(ch, 'unsubscribe_data', ch.id);
    },
  };
}
