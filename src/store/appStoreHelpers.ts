import { type Node, type Edge } from '@xyflow/react';
import { nanoid } from 'nanoid';
import { useAppStore } from './appStore';
import {
  makeTransportNodeDef,
  makeProtocolNodeDef,
  widgetToNodeKind,
  edgeToGraphEdge,
  type NodeDef,
} from '../lib/utils/nodeDef';
import { api } from '../lib/tauri/tauri';
import { notify } from '../lib/tauri/notifications';
import { nodeError } from '../lib/tauri/errorGuidance';
import { t } from '../i18n';
import {
  getEffectiveChannels,
  schemaFromProtocolConfig,
} from '../lib/utils/protocolSchema';
import type { WidgetConfig, ProtocolConfig, ProtocolSchema, TransportConfig } from '../types';

// getEffectiveChannels 已移至 lib/utils/protocolSchema (避免循环依赖), 此处 re-export 保持既有导入路径
export { getEffectiveChannels } from '../lib/utils/protocolSchema';

/// 节点错误通知文案 — 统一入口 (settings 开关在 errorGuidance 内读取)

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
  /// 数值输出口通道数 (ch0..chN) — 手动配置值或自动检测值 (预设路径用)
  channels: number;
  /// 协议帧 schema (协议 = 帧 schema; 预设为工厂产物, 用户编辑块后 preset='custom')
  /// 旧数据可能缺失 (快照迁移补齐), 消费方需按预设路径回退
  schema: ProtocolSchema;
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
      schema: schemaFromProtocolConfig(config),
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
///
/// 阶段二:ProtocolSource 注入下沉后端 (cmd_graph::inject_protocol_sources),
/// 前端不再手工推导 ProtocolSource NodeDef, 也无需按 protocolPortNames 判定注入条件。
/// 同步提交后端响应 GraphDerived 写入 `derivedPorts` store 作为端口渲染的单一权威。
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

  const nodes: NodeDef[] = [];
  // 1. widget 节点
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
  // 2. 全局节点定义 — 全部提交 (任何 tab 的 sync 都刷新全局表, 配置变更即时生效)
  for (const n of globalById.values()) {
    if (n.type === 'transport') {
      const data = n.data as TransportNodeData;
      nodes.push(makeTransportNodeDef(tabId, n.id, data.config));
    } else if (n.type === 'protocol') {
      const data = n.data as ProtocolNodeData;
      // 旧数据缺 schema 时按 config 回退构造 (快照迁移会补齐, 此处防御)
      const schema = data.schema ?? schemaFromProtocolConfig(data.config);
      // makeProtocolNodeDef 内部已强制 preset 时 schema=null (后端 schema 工厂下沉)
      nodes.push(makeProtocolNodeDef(tabId, n.id, data.config, data.convertTo ?? null, schema));
    }
  }

  const edges = tabEdges.map(edgeToGraphEdge);
  try {
    const derived = await api.updateTabGraph(tabId, nodes, edges);
    // 后端单一权威: 写入本次图变化涉及的节点派生数据
    if (derived?.nodes) state.setDerived(derived.nodes);
    // 图已变化 — 向所有已连接 Transport 推送最新下游协议 (热更新, 无需重连)
    refreshTransportProtocols();
  } catch (err) {
    const lang = useAppStore.getState().lang;
    notify.error(
      t(lang, 'notifNodeGraphSyncFailed'),
      nodeError(lang, err),
      { source: 'syncTabGraph' }
    );
  }
}

// ---- Transport 协议热更新 ----

let refreshTimer: ReturnType<typeof setTimeout> | null = null;

/// 图/协议变化后, 向所有已连接 Transport 推送其字节边下游的最新协议配置
/// (后端 TestData 生成器经 watch 通道热更新; 其他传输类型后端静默接受)。
/// 防抖合并: syncAllTabGraphs 会逐 tab 调用本函数。
export function refreshTransportProtocols(): void {
  if (refreshTimer) clearTimeout(refreshTimer);
  refreshTimer = setTimeout(() => {
    refreshTimer = null;
    void doRefreshTransportProtocols();
  }, 150);
}

async function doRefreshTransportProtocols(): Promise<void> {
  const state = useAppStore.getState();
  for (const n of state.rfNodes) {
    if (n.type !== 'transport') continue;
    if (state.connectionStates?.[n.id] !== 'Connected') continue;
    const downstreamId = downstreamProtocolOf(n.id, state.rfEdges, state.rfNodes);
    const protocolNode = downstreamId
      ? state.rfNodes.find((x) => x.id === downstreamId)
      : undefined;
    const protocol: ProtocolConfig = protocolNode
      ? (protocolNode.data as ProtocolNodeData).config
      : DEFAULT_PROTOCOL_CONFIG;
    // schema 一并下发 (TestData 生成器: custom 且带 encode 块时按 schema 编码)
    const schema = protocolNode
      ? ((protocolNode.data as ProtocolNodeData).schema ?? schemaFromProtocolConfig(protocol))
      : null;
    try {
      await api.updateTransportProtocol(n.id, protocol, schema);
    } catch (err) {
      // 热更新失败 (如连接已断开) — toast 提示用户手动重连
      const lang = useAppStore.getState().lang;
      notify.error(
        t(lang, 'notifTransportHotUpdateFailed'),
        nodeError(lang, err),
        {
          source: 'refreshTransportProtocols',
          actions: [
            {
              label: t(lang, 'notifReconnect'),
              run: () => { void useAppStore.getState().connectNode(n.id); },
            },
          ],
        }
      );
    }
  }
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

/// 从某节点沿字节边向上溯源, 找到第一个全局 Transport 节点 id
/// (RawData 单通道模式的发送目标 = 通道连线对应的串口; 找不到返回 null)
export function traceTransportSource(nodeId: string, edges: Edge[], nodes: Node[]): string | null {
  const transportIds = new Set(
    nodes.filter((n) => isGlobalNode(n) && n.type === 'transport').map((n) => n.id)
  );
  // 起点本身就是 Transport (通道 = 某串口的 rx 口) — 直接返回
  if (transportIds.has(nodeId)) return nodeId;
  // 字节平面源端口: Transport.rx / Protocol.out / CommandSender.loopbackOut / FrameDecoder.raw
  const BYTE_SOURCE_HANDLES = new Set(['rx', 'out', 'loopbackOut', 'raw']);
  const visited = new Set<string>();
  const stack = [nodeId];
  while (stack.length > 0) {
    const cur = stack.pop()!;
    if (visited.has(cur)) continue;
    visited.add(cur);
    if (transportIds.has(cur)) return cur;
    for (const e of edges) {
      if (e.target !== cur) continue;
      // 只沿字节边上溯 (数值口/控件输出不参与)
      if (!BYTE_SOURCE_HANDLES.has(e.sourceHandle ?? '')) continue;
      stack.push(e.source);
    }
  }
  return null;
}
