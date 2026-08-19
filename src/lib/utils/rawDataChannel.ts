import type { Node, Edge } from '@xyflow/react';
import type { WidgetConfig } from '../../types';
import { traceTransportSource } from '../../store/appStoreHelpers';

/// RawData 通道种类:
/// - decoder-node: FrameDecoder 的 raw 口 → 节点旁路收集器 (该解码器每帧消费的整帧字节)
/// - byte-source:  字节平面源 (Transport rx / Protocol out) → 上游 Transport 的原始收发字节流
/// - numeric:      其余数值源 → graphOutputs 数值流
export type RawDataChannelKind = 'decoder-node' | 'byte-source' | 'numeric';

export interface RawDataChannelInfo {
  kind: RawDataChannelKind;
  /// byte-source 通道的字节源 Transport 节点 id; 上溯失败为 null (通道显示为空, 不订阅)
  transportId: string | null;
}

/// 分类 RawData 控件的通道 (一条入边 = 一个通道), 决定该通道的数据来源
export function classifyRawDataChannel(
  channel: { sourceId: string; sourceHandle?: string },
  nodes: Node[],
  edges: Edge[],
  widgets: WidgetConfig[]
): RawDataChannelInfo {
  if (
    channel.sourceHandle === 'raw' &&
    widgets.some((w) => w.kind === 'FrameDecoder' && w.params.id === channel.sourceId)
  ) {
    return { kind: 'decoder-node', transportId: null };
  }
  const sourceNode = nodes.find((n) => n.id === channel.sourceId);
  if (sourceNode?.type === 'transport' || sourceNode?.type === 'protocol') {
    return {
      kind: 'byte-source',
      transportId: traceTransportSource(channel.sourceId, edges, nodes),
    };
  }
  return { kind: 'numeric', transportId: null };
}
