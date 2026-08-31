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
import type {
  DataTabMetaPayload,
  GraphSourceEventPayload,
  PositionPayload,
  SourceNodeHintPayload,
  TabMetaPayload,
  WidgetRecordPayload,
  WorkspaceSnapshotPayload,
} from '../lib/tauri/tauri';
import { notify } from '../lib/tauri/notifications';
import { nodeError } from '../lib/tauri/errorGuidance';
import { parseNodeError } from '../types/errors';
import { t } from '../i18n';
import {
  getEffectiveChannels,
  schemaFromProtocolConfig,
} from '../lib/utils/protocolSchema';
import { getWidgetPorts } from '../components/nodes/WidgetPorts';
import {
  normalizeModel3DConfig,
} from '../lib/utils/createWidget';
import { normalizeCommandConfig } from '../lib/utils/commandFrames';
import type {
  DataTab,
  WidgetConfig,
  ProtocolConfig,
  ProtocolSchema,
  TransportConfig,
} from '../types';

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
/// 连线拓扑的后端权威写入路径之一 (另两条: 拓扑 op connect/disconnect_edge 与
/// MCP update_graph, 三方共用 `apply_tab_graph_parts` 同一编译提交入口)。
/// - 提交载荷附带每节点端口提示 (后端拓扑 op 解析默认 handle / RawData 改写)
/// - 附带 base_version 乐观并发基线: 期间被拓扑 op / MCP 推进版本时收到
///   `GraphVersionConflict` → 拉取权威源图采纳合并后重试一次
/// - 成功响应写回新版本号; 编译失败 toast 提示 (真实原因) 并把文案返回给调用方
///
/// 返回: 用户可读的错误文案; 成功为 undefined
async function doSyncTabGraph(tabId: string, allowConflictRetry = true): Promise<string | undefined> {
  // 采纳护栏: 提交在途期间本 tab 的 graph:source 回声暂缓采纳 (在途提交的
  // 响应/冲突路径已收敛) — 否则回声会把画布刚落、尚未提交的变更覆盖掉
  activeSyncs.add(tabId);
  try {
    return await doSyncTabGraphOnce(tabId, allowConflictRetry);
  } finally {
    activeSyncs.delete(tabId);
  }
}

async function doSyncTabGraphOnce(
  tabId: string,
  allowConflictRetry: boolean
): Promise<string | undefined> {
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

  // 3. 端口提示 — widget 参数在前端, 后端拓扑 op 靠提示解析默认端口与 RawData 改写
  const nodeHints: Record<string, SourceNodeHintPayload> = {};
  for (const n of state.rfNodes) {
    if (isGlobalNode(n)) {
      nodeHints[n.id] =
        n.type === 'transport'
          ? { default_input: 'tx', default_output: 'rx' }
          : { default_input: 'in', default_output: 'out' };
      continue;
    }
    const widget = n.data?.widget as WidgetConfig | undefined;
    if (!widget) continue;
    const ports = getWidgetPorts(widget);
    nodeHints[n.id] = {
      ...(ports.inputs[0] ? { default_input: ports.inputs[0].id } : {}),
      ...(ports.outputs[0] ? { default_output: ports.outputs[0].id } : {}),
      ...(widget.kind === 'RawData' ? { raw_data: true } : {}),
    };
  }

  const edges = tabEdges.map(edgeToGraphEdge);

  // 4. widget 配置记录 + 画布位置 — 配置模型的后端权威存储 (随同一提交原子更新)
  const widgetRecords: WidgetRecordPayload[] = [];
  for (const n of state.rfNodes) {
    if (n.data?.tabId !== tabId) continue;
    const widget = n.data?.widget as WidgetConfig | undefined;
    if (!widget) continue;
    widgetRecords.push({
      id: n.id,
      kind: widget.kind,
      params: widget.params as unknown as Record<string, unknown>,
    });
  }
  const positions: Record<string, PositionPayload> = {};
  for (const n of state.rfNodes) {
    if (!tabNodeIds.has(n.id)) continue;
    positions[n.id] = { x: n.position.x, y: n.position.y };
  }

  try {
    const derived = await api.updateTabGraph(
      tabId,
      nodes,
      edges,
      nodeHints,
      state.graphVersion,
      widgetRecords,
      positions
    );
    // 后端单一权威: 版本号 + 本次图变化涉及的节点派生数据
    if (derived?.version != null) state.setGraphVersion(derived.version);
    if (derived?.nodes) state.setDerived(derived.nodes);
    // 图已变化 — 向所有已连接 Transport 推送最新下游协议 (热更新, 无需重连)
    refreshTransportProtocols();
    return undefined;
  } catch (err) {
    // 版本冲突: 期间有其他写入方 (拓扑 op / MCP) — 拉权威源图采纳合并后重试一次
    if (allowConflictRetry && isVersionConflictError(err)) {
      const source = await api.getSourceGraph(tabId).catch(() => null);
      if (source) {
        adoptSourceGraph(source);
        return doSyncTabGraph(tabId, false);
      }
    }
    const lang = useAppStore.getState().lang;
    const message = nodeError(lang, err);
    notify.error(
      t(lang, 'notifNodeGraphSyncFailed'),
      message,
      { source: 'syncTabGraph' }
    );
    return message;
  }
}

/// 后端 IPC 错误是否为图版本冲突 (GraphVersionConflict)
function isVersionConflictError(err: unknown): boolean {
  const data =
    err !== null && typeof err === 'object'
      ? ((err as Record<string, unknown>).data as { current?: unknown } | null)
      : null;
  if (data?.current != null) return true;
  return parseNodeError(err).message.includes('版本冲突');
}

// ---- graph:source 事件采纳 (画布 = 投影) ----

/// 每 tab 提交链 — 串行化同 tab 连发提交, 防止乱序整图替换互相覆盖
const syncChains = new Map<string, Promise<string | undefined>>();
/// 正在提交的 tab 集合 — graph:source 事件在提交在途时暂缓采纳 (由响应/冲突路径收敛)
const activeSyncs = new Set<string>();

/** 该 tab 是否有图提交在途 */
export function isSyncInFlight(tabId: string): boolean {
  return activeSyncs.has(tabId);
}

/// 同步指定 tab 的节点图到后端 — 同 tab 连发提交串行化
///
/// 返回用户可读错误文案 (成功为 undefined); 内部已 toast, 调用方按需消费返回值
/// (内置 AI 的画布操作工具把文案回传给模型自我修正)。需要严格时序的调用方
/// (删除 tab: 先重同步存活 tab 再移除) 可 await 本函数。
export function syncTabGraphToBackend(tabId: string): Promise<string | undefined> {
  const prev = syncChains.get(tabId) ?? Promise.resolve(undefined);
  // doSyncTabGraph 所有失败路径均已吞为返回值 — catch 仅兜底意外异常,
  // 同时保证等待方 (Promise.all) 永不 reject、无未处理拒绝
  const next = prev
    .then(() => doSyncTabGraph(tabId))
    .catch(() => undefined);
  syncChains.set(tabId, next);
  return next;
}

/// 已知控件 kind 全集 (编译期穷举 — 新增控件 kind 时此处必须同步,
/// 否则后端记录 / 快照中该类控件会被水合与采纳路径剔除)
const KNOWN_WIDGET_KINDS: Record<WidgetConfig['kind'], true> = {
  Knob: true,
  Button: true,
  Radio: true,
  Checkbox: true,
  Slider: true,
  Label: true,
  Waveform: true,
  PieChart: true,
  Image: true,
  Gauge: true,
  LED: true,
  NumberDisplay: true,
  Custom: true,
  Math: true,
  Filter: true,
  FFT: true,
  IFFT: true,
  Spectrum: true,
  Model3D: true,
  Command: true,
  FrameDecoder: true,
  TableView: true,
  RawData: true,
  Trigger: true,
  TextDisplay: true,
  TextInput: true,
  Str: true,
  TextOut: true,
};

/// JSON 值深比较 (键序无关) — 后端 serde_json 反序列化会对对象键排序,
/// 不能用字符串比较判断 widget 参数是否变化
function jsonDeepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((x, i) => jsonDeepEqual(x, b[i]));
  }
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  if (ka.length !== kb.length) return false;
  const objB = b as Record<string, unknown>;
  return ka.every((k) => k in objB && jsonDeepEqual((a as Record<string, unknown>)[k], objB[k]));
}

/// widget 配置记录 → WidgetConfig (归一化旧形态; 未知 kind 返回 null —
/// 调用方以记录原样构造占位节点, 画布仍可见、边端点仍存在)
export function widgetFromRecord(rec: WidgetRecordPayload): WidgetConfig | null {
  if (!(rec.kind in KNOWN_WIDGET_KINDS)) return null;
  const params = (rec.params ?? {});
  if (rec.kind === 'Command') {
    return { kind: 'Command', params: normalizeCommandConfig(params as never) };
  }
  if (rec.kind === 'Model3D') {
    return { kind: 'Model3D', params: normalizeModel3DConfig(params) };
  }
  return { kind: rec.kind, params } as unknown as WidgetConfig;
}

/// 记录 → 画布可渲染的 WidgetConfig (未知 kind 按原样落为占位控件 —
/// 后端是存储权威, 前端不认识也不丢弃)
export function widgetOrPlaceholder(rec: WidgetRecordPayload): WidgetConfig {
  return (
    widgetFromRecord(rec) ??
    ({
      kind: rec.kind,
      params: (rec.params ?? {}),
    } as unknown as WidgetConfig)
  );
}

/// 由 NodeDef 构造全局 Transport/Protocol 画布节点 (配置完整在 NodeDef 中;
/// 位置取工作区位置表, 缺省时按类型落默认位)。kind 非 Transport/Protocol 返回 null
export function globalNodeFromDef(
  def: GraphSourceEventPayload['nodes'][number],
  position?: PositionPayload
): Node | null {
  if (def.kind.kind === 'Transport') {
    const config = def.kind.params.config;
    if (!config) return null;
    return {
      id: def.id,
      type: 'transport',
      position: position ?? { x: 60, y: 60 },
      data: { global: true, config, label: config.kind } satisfies TransportNodeData,
    };
  }
  if (def.kind.kind === 'Protocol') {
    const config = def.kind.params.config as ProtocolConfig | undefined;
    if (!config) return null;
    return {
      id: def.id,
      type: 'protocol',
      position: position ?? { x: 300, y: 60 },
      data: {
        global: true,
        config,
        convertTo: (def.kind.params.convert_to as ProtocolConfig | null) ?? null,
        channels: getEffectiveChannels(config, null),
        schema:
          (def.kind.params.schema as ProtocolSchema | undefined) ??
          schemaFromProtocolConfig(config),
        label: config.kind,
      } satisfies ProtocolNodeData,
    };
  }
  return null;
}

/**
 * 采纳后端权威源图 — `graph:source` 事件 / 版本冲突重试共用
 *
 * 画布是源图的**纯投影**, 前端不做任何连线有效性判断 (编译权威在后端):
 * - 边: 该 tab 的边逐字替换为事件边集 — 后端编译认可的连线画布必须保留,
 *   不存在"前端认为悬空就删除"的路径;
 * - 全局节点: 缺失的 transport/protocol 自动补建 (配置完整在 NodeDef 中);
 * - widget: 事件携带配置记录时 (后端单一权威), 该 tab 的 widget 节点集合
 *   整体收敛到记录集 — 外部提交的纯 widget 图完整渲染, 外部删除同样生效;
 *   未知 kind 落为占位控件 (不丢弃); 事件未携带记录 (旧契约) 时画布 widget
 *   保持不动。
 * 事件携带位置表时, 已有节点的画布位置跟随更新。
 */
export function adoptSourceGraph(event: GraphSourceEventPayload): void {
  const state = useAppStore.getState();
  if (event.version) state.setGraphVersion(event.version);

  const positions = event.positions;
  const records = event.widgets;

  // 1. widget 配置记录收敛 — 新增 / 更新 / 删除 (记录集为该 tab 权威)
  const nodeUpdates = new Map<string, Node>();
  const widgetUpdates = new Map<string, WidgetConfig>();
  const addedWidgetNodes: Node[] = [];
  const addedWidgetConfigs: WidgetConfig[] = [];
  const removedWidgetIds = new Set<string>();
  let widgetSetChanged = false;

  if (records) {
    for (const rec of records) {
      const widget = widgetOrPlaceholder(rec);
      const existing = state.rfNodes.find((n) => n.id === rec.id);
      const pos = positions?.[rec.id];
      if (existing) {
        const cur = existing.data?.widget as WidgetConfig | undefined;
        const configChanged =
          !cur || cur.kind !== widget.kind || !jsonDeepEqual(cur.params, widget.params);
        const posChanged =
          pos != null && (existing.position.x !== pos.x || existing.position.y !== pos.y);
        if (configChanged || posChanged) {
          nodeUpdates.set(rec.id, {
            ...existing,
            position: pos ?? existing.position,
            data: { ...existing.data, widget },
          });
          widgetUpdates.set(rec.id, widget);
          widgetSetChanged = true;
        }
      } else {
        addedWidgetNodes.push({
          id: rec.id,
          type: 'widget',
          position: pos ?? { x: 240 + Math.random() * 100, y: 80 + Math.random() * 80 },
          data: { widget, tabId: event.tab_id },
        });
        addedWidgetConfigs.push(widget);
        widgetSetChanged = true;
      }
    }
    const recordIds = new Set(records.map((r) => r.id));
    for (const n of state.rfNodes) {
      if (!isGlobalNode(n) && n.data?.tabId === event.tab_id && !recordIds.has(n.id)) {
        removedWidgetIds.add(n.id);
        widgetSetChanged = true;
      }
    }
  }

  // 2. 补建缺失的全局节点 (id 未出现过的 transport/protocol)
  const knownIds = new Set(state.rfNodes.map((n) => n.id));
  const addedGlobalNodes: Node[] = [];
  for (const def of event.nodes) {
    if (knownIds.has(def.id)) continue;
    const node = globalNodeFromDef(def, positions?.[def.id]);
    if (node) addedGlobalNodes.push(node);
  }

  // 3. 位置跟随 — 已存在节点的画布位置按事件位置表更新
  const posUpdates = new Map<string, Node>();
  if (positions) {
    for (const n of state.rfNodes) {
      if (nodeUpdates.has(n.id)) continue; // 配置更新已携带最新位置
      const pos = positions[n.id];
      if (!pos || (n.position.x === pos.x && n.position.y === pos.y)) continue;
      posUpdates.set(n.id, { ...n, position: pos });
    }
  }

  // 4. 该 tab 的边逐字采纳 — 后端编译认可即保留, 前端不判断端口有效性;
  //    仅按 tab 归属划分替换范围 (其他 tab 的边不动)
  const keptNodes = removedWidgetIds.size
    ? state.rfNodes.filter((n) => !removedWidgetIds.has(n.id))
    : state.rfNodes;
  const scopedNodes = [
    ...keptNodes,
    ...nodeUpdates.values(),
    ...posUpdates.values(),
    ...addedWidgetNodes,
    ...addedGlobalNodes,
  ];
  const tabNodeIds = new Set(
    scopedNodes
      .filter((n) => isGlobalNode(n) || n.data?.tabId === event.tab_id)
      .map((n) => n.id)
  );
  const isTabEdge = (e: { source: string; target: string }) =>
    tabNodeIds.has(e.source) && tabNodeIds.has(e.target);

  const adopted: Edge[] = event.edges.map((e) => ({
    id: e.id,
    source: e.source,
    sourceHandle: e.source_handle,
    target: e.target,
    targetHandle: e.target_handle,
  }));

  const keyOf = (e: Edge) =>
    `${e.id}|${e.source}|${e.sourceHandle ?? ''}|${e.target}|${e.targetHandle ?? ''}`;
  const oldEdges = removedWidgetIds.size
    ? state.rfEdges.filter((e) => !removedWidgetIds.has(e.source) && !removedWidgetIds.has(e.target))
    : state.rfEdges;
  const oldKeys = new Set(oldEdges.filter(isTabEdge).map(keyOf));
  const edgesChanged =
    oldKeys.size !== adopted.length || adopted.some((e) => !oldKeys.has(keyOf(e)));
  if (
    !edgesChanged &&
    addedGlobalNodes.length === 0 &&
    !widgetSetChanged &&
    posUpdates.size === 0
  ) {
    return;
  }

  // 应用: 节点 = 保留集 ⊕ 更新 ⊕ 新增; widgets 平铺数组与 tab 隶属同步收敛
  const updateMap = new Map<string, Node>([...nodeUpdates, ...posUpdates]);
  const rfNodes = [
    ...keptNodes.map((n) => updateMap.get(n.id) ?? n),
    ...addedWidgetNodes,
    ...addedGlobalNodes,
  ];
  const others = oldEdges.filter((e) => !isTabEdge(e));
  let widgets = state.widgets;
  if (removedWidgetIds.size) widgets = widgets.filter((w) => !removedWidgetIds.has(w.params.id));
  if (widgetUpdates.size) {
    // 平铺数组 upsert: 条目在则更新, 不在 (历史不一致) 则补齐, 与画布保持一致
    const touched = new Set<string>();
    widgets = widgets.map((w) => {
      const nw = widgetUpdates.get(w.params.id);
      if (!nw) return w;
      touched.add(w.params.id);
      return nw;
    });
    const missing = [...widgetUpdates].filter(([id]) => !touched.has(id));
    if (missing.length) widgets = [...widgets, ...missing.map(([, w]) => w)];
  }
  if (addedWidgetConfigs.length) widgets = [...widgets, ...addedWidgetConfigs];
  let controlTabs = state.controlTabs;
  if (records) {
    const liveIds = new Set(widgets.map((w) => w.params.id));
    const ordered = records.map((r) => r.id).filter((id) => liveIds.has(id));
    controlTabs = controlTabs.map((t) =>
      t.id === event.tab_id ? { ...t, widgets: ordered } : t
    );
  }
  useAppStore.setState({ rfNodes, rfEdges: [...others, ...adopted], widgets, controlTabs });
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
      const transport = (n.data as TransportNodeData).config;
      await api.updateTransportProtocol(
        n.id,
        protocol,
        schema,
        transport.kind === 'TestData' ? transport.params : null,
      );
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

// ---- 启动水合 (workspace_get) + tab 元数据同步 ----

/// 从后端水合工作区快照 — 启动时调用一次
///
/// 返回是否水合了持久化工作区: false = 无持久化 (全新安装 / 后端不可达),
/// 前端保持默认状态并走初始同步 + 种子流程; true = 本地已被后端权威快照
/// 覆盖 (控件 tab / 数据面板 / widget 配置 / 画布 / 边 / 版本基线),
/// 后端启动时已完成逐 tab 重编译, 不再初始同步。
export async function hydrateWorkspaceFromBackend(): Promise<boolean> {
  let snap: WorkspaceSnapshotPayload | null = null;
  try {
    snap = await api.workspaceGet();
  } catch {
    return false;
  }
  if (!snap) return false;

  // 1. 全部 tab 源图 → widget 节点 + 全局节点 + 边 (先建节点, 再按端点全集过滤边)
  const widgetNodes: Node[] = [];
  const globalNodes = new Map<string, Node>();
  const widgets: WidgetConfig[] = [];
  for (const g of snap.graphs) {
    for (const def of g.nodes) {
      if (globalNodes.has(def.id)) continue;
      if (def.kind.kind !== 'Transport' && def.kind.kind !== 'Protocol') continue;
      const node = globalNodeFromDef(def, snap.positions[def.id]);
      if (node) globalNodes.set(def.id, node);
    }
    for (const rec of g.widgets) {
      // 未知 kind 落为占位控件 — 画布是后端状态的投影, 不因前端不认识而丢弃
      const widget = widgetOrPlaceholder(rec);
      widgets.push(widget);
      widgetNodes.push({
        id: rec.id,
        type: 'widget',
        position: snap.positions[rec.id] ?? { x: 240, y: 80 },
        data: { widget, tabId: g.tab_id },
      });
    }
  }
  const localIds = new Set([...globalNodes.keys(), ...widgetNodes.map((n) => n.id)]);
  const edges: Edge[] = [];
  for (const g of snap.graphs) {
    for (const e of g.edges) {
      if (!localIds.has(e.source) || !localIds.has(e.target)) continue;
      edges.push({
        id: e.id,
        source: e.source,
        sourceHandle: e.source_handle,
        target: e.target,
        targetHandle: e.target_handle,
      });
    }
  }

  // 2. tab 元数据 (控件 tab 至少保留一个; 数据面板兜底注入 fixed 两页 —
  //    与备份快照应用同规则)
  const controlTabs = (snap.tabs.length
    ? snap.tabs
    : [{ id: 'default', name: 'Tab 1', widgets: [] }]
  ).map((t) => ({ id: t.id, name: t.name, widgets: t.widgets ?? [] }));
  const dataTabs: DataTab[] = snap.data_tabs.map((t) => ({
    id: t.id,
    type: t.type as DataTab['type'],
    name: t.name,
    closable: t.closable,
    ...(t.widget_id != null ? { widgetId: t.widget_id } : {}),
  }));
  for (const ft of [
    { id: 'compile-errors-fixed', type: 'compile-errors', name: 'Compile Errors', closable: false },
    { id: 'compile-results-fixed', type: 'compile-results', name: 'Compile Results', closable: false },
  ] as DataTab[]) {
    if (!dataTabs.some((t) => t.id === ft.id)) dataTabs.push(ft);
  }

  useAppStore.setState({
    controlTabs,
    activeControlTabId: controlTabs[0].id,
    dataTabs,
    activeDataTabId: dataTabs[0]?.id ?? 'compile-results-fixed',
    widgets,
    rfNodes: [...globalNodes.values(), ...widgetNodes],
    rfEdges: edges,
    graphVersion: snap.version ?? null,
  });
  return true;
}

/// tab 元数据整表覆盖到后端工作区 (控件 tab / 数据面板增删改名后由订阅触发)
export function syncWorkspaceMeta(): void {
  const state = useAppStore.getState();
  const tabs: TabMetaPayload[] = state.controlTabs.map((t) => ({
    id: t.id,
    name: t.name,
    widgets: [...t.widgets],
  }));
  const dataTabs: DataTabMetaPayload[] = state.dataTabs.map((t) => ({
    id: t.id,
    name: t.name,
    type: t.type,
    closable: t.closable,
    ...(t.widgetId != null ? { widget_id: t.widgetId } : {}),
  }));
  void api.workspaceSetTabs(tabs, dataTabs).catch(() => {});
}
