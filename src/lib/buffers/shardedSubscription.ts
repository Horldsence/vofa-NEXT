import { invoke, Channel } from '@tauri-apps/api/core';
import { closeTauriChannel } from '../tauri/tauri';
import { tickMetric, perfEvent } from '../utils/perfLog';

/// 分片池大小 — 与后端 pipeline::stream::MAX_STREAM_SHARDS 一致
export const STREAM_SHARDS = 4;

interface SeqBatch {
  seq: number;
}

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
  intervalMs?: number
): { cancel: () => void } {
  const channel = new Channel<DisplayEvent<T>>();
  channel.onmessage = (event) => {
    if (event.kind === expectedKind) sink(event.payload);
  };
  void invoke<string>('subscribe_display', {
    request,
    onEvent: channel,
    intervalMs,
    groupId: null,
    maxItems: null,
  });
  return {
    cancel: () => void closeTauriChannel(channel, 'unsubscribe_display', channel.id),
  };
}

/// seq 严格重组 (增量流: RAWDATA/CAN/逻辑/解码)
/// 分片并发推送的批次可能乱序到达, 按组级单调 seq 排序后交付,
/// 保证顺序与后端 drain 顺序一致
export function makeOrderedSink<T extends SeqBatch>(deliver: (batch: T) => void) {
  let next = 0;
  const pending = new Map<number, T>();
  return (batch: T) => {
    if (batch.seq < next) return; // 过期/重复批次
    pending.set(batch.seq, batch);
    tickMetric('orderedSink:pending', 0, pending.size);
    // 防 gap 卡死: 某分片异常退出导致 seq 缺失时, 积压超阈值跳到最小可用序号
    if (pending.size > 64) {
      next = Math.min(...pending.keys());
    }
    while (pending.has(next)) {
      deliver(pending.get(next)!);
      pending.delete(next);
      next++;
    }
  };
}

/// 最新 seq 胜出 (快照流: 波形)
/// 只交付 seq 更新的快照, 乱序到达的旧快照直接丢弃
export function makeLatestSink<T extends SeqBatch>(deliver: (batch: T) => void) {
  let latest = -1;
  return (batch: T) => {
    if (batch.seq <= latest) return;
    latest = batch.seq;
    deliver(batch);
  };
}

/// 统一分片订阅 — 所有数据流 (RAWDATA/波形/CAN/逻辑/解码) 共用
///
/// 一次性建立 STREAM_SHARDS 个 Channel 组成订阅组: 首个 invoke 创建组并返回组 id,
/// 其余凭组 id 加入。后端按积压自动激活分片 — 单 channel 够用时只有 shard 0 工作,
/// 不够时自动多通道并行推送。
///
/// - cmd / unsubscribeCmd: 订阅/取消命令名
/// - extraArgs: 额外 invoke 参数 (如 nodeId)
/// - sink: makeOrderedSink (增量流) 或 makeLatestSink (快照流) 包装后的交付函数
/// - options: 透传的 intervalMs / maxXxx 参数 (key 需与后端参数驼峰对应)
///
/// 若首个 invoke 返回空组 id (如 FrameDecoder 节点不存在的 no-op), 不再加入额外分片。
/// 返回取消函数 (取消全部分片)
export function subscribeDisplaySharded<T extends SeqBatch>(
  request: Record<string, unknown>,
  expectedKind: DisplayKind,
  sink: (batch: T) => void,
  options?: { intervalMs?: number; maxItems?: number }
): { cancel: () => void } {
  const channels: Channel<DisplayEvent<T>>[] = [];
  let cancelled = false;

  // 调试: 统计该订阅的消息速率 (payload 字节由各订阅包装补充)
  const countedSink = (event: DisplayEvent<T>) => {
    if (event.kind !== expectedKind) {
      console.error(`显示订阅类型不匹配: expected=${expectedKind}, actual=${event.kind}`);
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
      const groupId = await invoke<string>('subscribe_display', {
        request,
        onEvent: first,
        ...options,
      });
      if (!groupId) return; // 后端 no-op (如节点不存在)
      for (let i = 1; i < STREAM_SHARDS; i++) {
        if (cancelled) break;
        const ch = new Channel<DisplayEvent<T>>();
        ch.onmessage = countedSink;
        channels.push(ch);
        await invoke<string>('subscribe_display', {
          request,
          onEvent: ch,
          groupId,
          ...options,
        });
        // 取消后仍在建的 channel: 立即关闭, 避免后端任务泄漏
        if (cancelled) {
          void closeTauriChannel(ch, 'unsubscribe_display', ch.id);
        }
      }
    } catch (e) {
      console.error(`显示订阅失败 (${expectedKind}):`, e);
    }
  })();

  return {
    cancel: () => {
      cancelled = true;
      perfEvent(`cancel display:${expectedKind} (${channels.length} channels)`);
      for (const ch of channels) {
        void closeTauriChannel(ch, 'unsubscribe_display', ch.id);
      }
    },
  };
}
