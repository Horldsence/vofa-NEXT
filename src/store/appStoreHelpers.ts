import { type Node, type Edge } from '@xyflow/react';
import { nanoid } from 'nanoid';
import { useAppStore } from './appStore';
import {
  makeProtocolSourceNodeDef,
  makeTransportNodeDef,
  makeProtocolNodeDef,
  widgetToNodeKind,
  edgeToGraphEdge,
  type NodeDef,
} from '../lib/utils/nodeDef';
import { api } from '../lib/tauri/tauri';
import { notify, formatError } from '../lib/tauri/notifications';
import { t } from '../i18n';
import type { WidgetConfig, ProtocolConfig, TransportConfig } from '../types';

/// 全局节点 (Transport/Protocol) data 公共字段
export interface GlobalNodeData {
  global: true;
  label: string;
  [key: string]: unknown;
}

export interface TransportNodeData extends GlobalNodeData {
  config: TransportConfig;
}

export interface ProtocolNodeData extends GlobalNodeData {
  config: ProtocolConfig;
  /// 可选协议转换目标 (null = 无转换)
  convertTo: ProtocolConfig | null;
  /// 数值输出口通道数 (ch0..chN) — 手动配置值或自动检测值
  channels: number;
}

/// 各传输类型的默认配置 (与旧 TransportConfigPanel.switchKind 一致)
export function defaultTransportConfig(kind: TransportConfig['kind']): TransportConfig {
  switch (kind) {
    case 'Serial':
      return {
        kind: 'Serial',
        params: {
          port_name: '',
          baud_rate: 115200,
          data_bits: 8,
          parity: 'none',
          stop_bits: 'one',
          flow_control: 'none',
        },
      };
    case 'Udp':
      return {
        kind: 'Udp',
        params: {
          local_addr: '0.0.0.0',
          remote_addr: '127.0.0.1',
          local_port: 8888,
          remote_port: 9999,
        },
      };
    case 'TcpClient':
      return { kind: 'TcpClient', params: { host: '127.0.0.1', port: 8080 } };
    case 'TcpServer':
      return { kind: 'TcpServer', params: { listen_addr: '0.0.0.0', listen_port: 8080 } };
    case 'TestData':
      return { kind: 'TestData', params: { channels: 4, sample_rate: 100, signal: 'Sine' } };
    case 'Slcan':
      return { kind: 'Slcan', params: { port_name: '', baud_rate: 115200, can_bitrate: 'bps500k' } };
    case 'CandleLight':
      return { kind: 'CandleLight', params: { bus: 1, address: 0, can_bitrate: 'bps500k', channel: 0 } };
  }
}

export const DEFAULT_PROTOCOL_CONFIG: ProtocolConfig = { kind: 'JustFloat', channels: null };

/// 创建 Transport 全局节点 (渲染在所有 tab 画布上)
export function createTransportNode(
  kind: TransportConfig['kind'],
  position?: { x: number; y: number }
): Node {
  const config = defaultTransportConfig(kind);
  return {
    id: `transport-${nanoid(8)}`,
    type: 'transport',
    position: position ?? { x: 60, y: 60 },
    data: { global: true, config, label: kind } satisfies TransportNodeData,
  };
}

/// 创建 Protocol 全局节点 (渲染在所有 tab 画布上)
export function createProtocolNode(
  config: ProtocolConfig = DEFAULT_PROTOCOL_CONFIG,
  position?: { x: number; y: number }
): Node {
  return {
    id: `protocol-${nanoid(8)}`,
    type: 'protocol',
    position: position ?? { x: 300, y: 60 },
    data: {
      global: true,
      config,
      convertTo: null,
      channels: getEffectiveChannels(config, null),
      label: config.kind,
    } satisfies ProtocolNodeData,
  };
}

/// 节点是否全局字节平面节点 (Transport/Protocol)
export function isGlobalNode(n: Node): boolean {
  return n.data?.global === true;
}

/// 同步指定 tab 的节点图到后端
///
/// 收集:
/// - 本 tab 的 widget 节点 (widgetToNodeKind)
/// - 本 tab 边引用的全局 Transport/Protocol 节点 (字节平面定义, 原样提交字节边)
/// - ProtocolSource 转换: 本 tab 内有边从某全局 Protocol 节点的 chN 端口发出时,
///   追加一个 ProtocolSource NodeDef (id = 全局 Protocol 节点 id)
///
/// 注意: ProtocolSource 定义排在全局 Transport/Protocol 定义之前 — 当前后端
/// update_tab_graph 按 id 覆盖合并, 同 id 时后者生效, 保证字节平面 Protocol 定义存活
/// (数值平面的 ch 槽位由后端 ProtocolSource 处理, 见后端 reconcile 工作)。
export async function syncTabGraphToBackend(tabId: string): Promise<void> {
  const state = useAppStore.getState();
  // 本 tab 可见节点 = 本 tab widget 节点 + 全部全局节点
  const tabNodeIds = new Set(
    state.rfNodes
      .filter((n) => n.data?.tabId === tabId || isGlobalNode(n))
      .map((n) => n.id)
  );
  // 本 tab 的边: 两端都在可见集合内
  const tabEdges = state.rfEdges.filter((e) => tabNodeIds.has(e.source) && tabNodeIds.has(e.target));

  const globalById = new Map(state.rfNodes.filter(isGlobalNode).map((n) => [n.id, n]));

  // 本 tab 引用的 ProtocolSource (Protocol 节点 id → 最大通道号)
  const protocolSources = new Map<string, number>();
  for (const e of tabEdges) {
    const src = globalById.get(e.source);
    if (src?.type !== 'protocol') continue;
    const m = /^ch(\d+)$/.exec(e.sourceHandle ?? '');
    if (!m) continue;
    const ch = parseInt(m[1], 10) + 1;
    protocolSources.set(e.source, Math.max(protocolSources.get(e.source) ?? 0, ch));
  }

  const nodes: NodeDef[] = [];
  // 1. ProtocolSource 定义 (须在全局定义之前, 见函数头注释)
  for (const [pid, maxCh] of protocolSources) {
    const data = globalById.get(pid)?.data as ProtocolNodeData | undefined;
    const channels = Math.max(data?.channels ?? 0, maxCh);
    nodes.push(makeProtocolSourceNodeDef(tabId, pid, channels));
  }
  // 2. widget 节点
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
  // 3. 全局节点定义 — 全部提交 (任何 tab 的 sync 都刷新全局表, 配置变更即时生效)
  for (const n of globalById.values()) {
    if (n.type === 'transport') {
      const data = n.data as TransportNodeData;
      nodes.push(makeTransportNodeDef(tabId, n.id, data.config));
    } else if (n.type === 'protocol') {
      const data = n.data as ProtocolNodeData;
      nodes.push(makeProtocolNodeDef(tabId, n.id, data.config, data.convertTo ?? null));
    }
  }

  const edges = tabEdges.map(edgeToGraphEdge);
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

/// 从某节点沿数值边向上溯源, 找到第一个全局 Protocol 节点 id (波形等 Sink 的数据源)
/// 无连接或溯源不到时返回 null
export function traceProtocolSource(nodeId: string, edges: Edge[], nodes: Node[]): string | null {
  const globalProtocolIds = new Set(
    nodes.filter((n) => isGlobalNode(n) && n.type === 'protocol').map((n) => n.id)
  );
  const visited = new Set<string>();
  const stack = [nodeId];
  while (stack.length > 0) {
    const cur = stack.pop()!;
    if (visited.has(cur)) continue;
    visited.add(cur);
    if (cur !== nodeId && globalProtocolIds.has(cur)) return cur;
    for (const e of edges) {
      if (e.target !== cur) continue;
      // 字节边不参与数值溯源
      const sh = e.sourceHandle ?? '';
      if (sh === 'loopbackOut' || sh === 'rx' || sh === 'out') continue;
      stack.push(e.source);
    }
  }
  return null;
}

/// 找到某 Transport 节点沿字节边下游的第一个 Protocol 节点 id
export function downstreamProtocolOf(transportNodeId: string, edges: Edge[], nodes: Node[]): string | null {
  const protocolIds = new Set(
    nodes.filter((n) => isGlobalNode(n) && n.type === 'protocol').map((n) => n.id)
  );
  const visited = new Set<string>();
  const stack = [transportNodeId];
  while (stack.length > 0) {
    const cur = stack.pop()!;
    if (visited.has(cur)) continue;
    visited.add(cur);
    if (cur !== transportNodeId && protocolIds.has(cur)) return cur;
    for (const e of edges) {
      if (e.source === cur) stack.push(e.target);
    }
  }
  return null;
}
