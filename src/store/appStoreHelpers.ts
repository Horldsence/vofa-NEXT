import { type Node } from '@xyflow/react';
import { useAppStore } from './appStore';
import { makeChannelSourceNodeDef, widgetToNodeKind, edgeToGraphEdge, type NodeDef } from '../lib/nodeDef';
import { api } from '../lib/tauri';
import { notify, formatError } from '../lib/notifications';
import { t } from '../i18n';
import type { WidgetConfig, ProtocolConfig } from '../types';

/// 通道源节点 ID (全局唯一, 不可删除)
export const CHANNEL_SOURCE_ID = '__channel_source__';

/// 创建通道源节点 (每个 tab 一个)
export function createChannelSourceNode(tabId: string, channelCount: number): Node {
  return {
    id: `${CHANNEL_SOURCE_ID}-${tabId}`,
    type: 'channelSource',
    position: { x: 40, y: 40 },
    data: { tabId, channelCount, label: 'Channel Source' },
    selectable: false,
    deletable: false,
  };
}

/// 同步指定 tab 的节点图到后端
/// 收集该 tab 的所有节点 (ChannelSource + widgets) 与边, 整体替换后端图
export async function syncTabGraphToBackend(tabId: string): Promise<void> {
  const state = useAppStore.getState();
  const tabNodeIds = new Set(
    state.rfNodes
      .filter((n) => n.data?.tabId === tabId || n.id === `${CHANNEL_SOURCE_ID}-${tabId}`)
      .map((n) => n.id)
  );
  // 收集 NodeDef: ChannelSource + widgets
  const nodes: NodeDef[] = [];
  const channelSourceNode = state.rfNodes.find(
    (n) => n.id === `${CHANNEL_SOURCE_ID}-${tabId}` && n.type === 'channelSource'
  );
  if (channelSourceNode) {
    const data = channelSourceNode.data as { channelCount?: number } | undefined;
    const chCount: number = data?.channelCount ?? 4;
    nodes.push(makeChannelSourceNodeDef(tabId, chCount));
  }
  for (const n of state.rfNodes) {
    if (n.data?.tabId !== tabId) continue;
    const widget = n.data?.widget as WidgetConfig | undefined;
    if (!widget) continue;
    nodes.push({
      id: n.id,
      tab_id: tabId,
      kind: widgetToNodeKind(widget),
    });
  }
  // 收集 tab 内的 edges (source 和 target 都在 tab 内)
  const edges = state.rfEdges
    .filter((e) => tabNodeIds.has(e.source) && tabNodeIds.has(e.target))
    .map(edgeToGraphEdge);
  try {
    await api.updateTabGraph(tabId, nodes, edges);
  } catch (err) {
    const lang = useAppStore.getState().lang;
    notify.error(
      t(lang, 'notifNodeGraphSyncFailed'),
      formatError(err),
      { source: 'syncTabGraph' }
    );
  }
}

/// 获取当前生效通道数 (优先检测值, 其次配置值)
export function getEffectiveChannels(
  protocolConfig: ProtocolConfig,
  detectedChannels: number | null
): number {
  if (protocolConfig.kind === 'RawData' || protocolConfig.kind === 'Slcan' || protocolConfig.kind === 'CandleLight' || protocolConfig.kind === 'LogicDecode') return 4;
  const configured = protocolConfig.channels;
  if (configured != null) return configured;
  return detectedChannels ?? 4;
}
