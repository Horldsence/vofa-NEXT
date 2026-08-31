//! AI 前端托管工具宿主 — 内置 AI 节点编辑 / UI 操作的前端执行端。
//!
//! 后端 `NativeToolExecutor` 对前端托管工具发出 `ai_tool_invoke` 事件,本模块
//! 分发到对应 handler (全部走 `useAppStore` 现有 action:画布实时刷新、撤销
//! 历史可用、tab 图自动同步后端),执行后 `ai_tool_resolve` 回执;未回执时
//! 后端 15s 超时报错兜底。
//!
//! 连线拓扑 (connect_nodes / disconnect_edge) 已下沉后端权威
//! (`cmd_graph::source_graph`),由内置 AI 后端直连执行,不走本模块 —
//! 编译失败 (端口域不匹配/成环) 直接报错且不建边,画布经 `graph:source`
//! 事件收敛 (见 appStoreHelpers.adoptSourceGraph)。

import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../store/appStore';
import type { Node } from '@xyflow/react';
import { createWidget } from '../utils/createWidget';
import {
  isGlobalNode,
  type ProtocolNodeData,
  type TransportNodeData,
} from '../../store/appStoreHelpers';
import { nodePortTable } from '../utils/connectionRules';
import { QUICK_START_TEMPLATES, getTemplate } from '../quickstart/templates';
import { applyTemplate, type TemplateApplyMode } from '../quickstart/applyTemplate';
import { t } from '../../i18n';
import type { ProtocolConfig, TransportConfig, WidgetConfig } from '../../types';

/// 后端调用载荷 (`cmd_ai::native_executor` 同构)。
interface AiToolInvoke {
  call_id: string;
  name: string;
  arguments: Record<string, unknown>;
}

/// 合法 widget kind 清单 (与 `WidgetConfig` 联合类型一致, 供运行时校验)。
const WIDGET_KINDS = [
  'Knob', 'Button', 'Radio', 'Checkbox', 'Slider', 'Label', 'Waveform',
  'PieChart', 'Image', 'Gauge', 'LED', 'NumberDisplay', 'Custom', 'Math',
  'Filter', 'FFT', 'IFFT', 'Spectrum', 'Model3D', 'Command', 'FrameDecoder',
  'TableView', 'RawData', 'Trigger', 'TextDisplay', 'TextInput', 'Str', 'TextOut',
] as const;

/// 合法 transport / protocol kind 清单。
const TRANSPORT_KINDS = ['Serial', 'Udp', 'TcpClient', 'TcpServer', 'TestData', 'Slcan', 'CandleLight'] as const;
const PROTOCOL_KINDS = ['JustFloat', 'FireWater', 'RawData', 'Slcan', 'CandleLight', 'LogicDecode'] as const;

/** 工具 handler — 返回值作为工具结果回传 (JSON 序列化);抛错即工具失败。 */
type ToolHandler = (args: Record<string, unknown>) => Promise<unknown> | unknown;

// ============ 参数辅助 ============

function str(args: Record<string, unknown>, key: string): string {
  const v = args[key];
  if (typeof v !== 'string' || v === '') throw new Error(`缺少字符串参数 ${key}`);
  return v;
}

function optStr(args: Record<string, unknown>, key: string): string | undefined {
  const v = args[key];
  return typeof v === 'string' && v !== '' ? v : undefined;
}

function obj(args: Record<string, unknown>, key: string): Record<string, unknown> {
  const v = args[key];
  return v !== null && typeof v === 'object' && !Array.isArray(v)
    ? (v as Record<string, unknown>)
    : {};
}

function positionOf(args: Record<string, unknown>): { x: number; y: number } | undefined {
  const p = args.position;
  if (p === null || typeof p !== 'object') return undefined;
  const { x, y } = p as Record<string, unknown>;
  if (typeof x !== 'number' || typeof y !== 'number') return undefined;
  return { x, y };
}

/** 深合并 patch 到 base (对象递归合并, 数组/标量整体替换)。 */
function deepMerge<T>(base: T, patch: unknown): T {
  if (
    patch === null || typeof patch !== 'object' || Array.isArray(patch) ||
    base === null || typeof base !== 'object' || Array.isArray(base)
  ) {
    return patch as T;
  }
  const out: Record<string, unknown> = { ...(base as Record<string, unknown>) };
  for (const [k, v] of Object.entries(patch as Record<string, unknown>)) {
    out[k] = k in out ? deepMerge(out[k], v) : v;
  }
  return out as T;
}

/** 按 id 找节点, 不存在抛错。 */
function nodeOrThrow(nodeId: string): Node {
  const node = useAppStore.getState().rfNodes.find((n) => n.id === nodeId);
  if (!node) throw new Error(`节点不存在: ${nodeId} (先 get_workspace 查询有效 id)`);
  return node;
}

/** 构造协议配置 (kind 不可变, JustFloat/FireWater 支持 channels)。 */
function buildProtocolConfig(kind: string, config: unknown): ProtocolConfig {
  const c =
    config !== null && typeof config === 'object' && !Array.isArray(config)
      ? (config as Record<string, unknown>)
      : {};
  if (kind === 'JustFloat' || kind === 'FireWater') {
    const channels = typeof c.channels === 'number' ? c.channels : null;
    return { kind, channels };
  }
  // RawData/Slcan/CandleLight/LogicDecode — 字段透传 (如 LogicDecode.decoder)
  return { kind, ...c } as unknown as ProtocolConfig;
}

/** Transport 配置合并: kind 变化时整体替换, 否则 params 深合并。 */
function mergeTransportConfig(
  current: TransportConfig,
  patch: Record<string, unknown>
): TransportConfig {
  if (typeof patch.kind === 'string' && patch.kind !== current.kind) {
    return patch as unknown as TransportConfig;
  }
  const params = (patch.params ?? {}) as Record<string, unknown>;
  return { ...current, params: deepMerge(current.params, params) } as TransportConfig;
}

/** 节点端口表 (供 get_workspace 暴露给 AI) — 与画布校验同一数据源。 */
function portsOf(node: Node): unknown {
  const s = useAppStore.getState();
  return nodePortTable(node, {
    derivedPorts: s.derivedPorts,
    detectedChannels: s.detectedChannels,
  });
}

// ============ Handlers ============

/** 读取画布全量状态 — AI 了解当前工作区的唯一权威来源。 */
function getWorkspace(): unknown {
  const s = useAppStore.getState();
  return {
    active_tab_id: s.activeControlTabId,
    tabs: s.controlTabs.map((tab) => ({
      id: tab.id,
      name: tab.name,
      widget_count: tab.widgets.length,
    })),
    widgets: s.rfNodes
      .filter((n) => !isGlobalNode(n) && n.data?.widget)
      .map((n) => {
        const widget = n.data.widget as WidgetConfig;
        return {
          id: n.id,
          tab_id: n.data.tabId,
          kind: widget.kind,
          label: (widget.params as { label?: string }).label,
          position: n.position,
          config: widget.params,
          ports: portsOf(n),
        };
      }),
    global_nodes: s.rfNodes.filter(isGlobalNode).map((n) => {
      if (n.type === 'transport') {
        const data = n.data as unknown as TransportNodeData;
        return {
          id: n.id,
          node_type: 'transport',
          kind: data.config.kind,
          config: data.config.params,
          position: n.position,
          connection: s.connectionStates[n.id] ?? 'Disconnected',
          ports: portsOf(n),
        };
      }
      const data = n.data as unknown as ProtocolNodeData;
      return {
        id: n.id,
        node_type: 'protocol',
        kind: data.config.kind,
        config: data.config,
        convert_to: data.convertTo,
        channels: data.channels,
        position: n.position,
        ports: portsOf(n),
      };
    }),
    edges: s.rfEdges.map((e) => ({
      id: e.id,
      source: e.source,
      source_handle: e.sourceHandle,
      target: e.target,
      target_handle: e.targetHandle,
    })),
    derived_ports: s.derivedPorts,
  };
}

/** 添加节点 (transport / protocol / widget)。 */
function addNode(args: Record<string, unknown>): unknown {
  const type = str(args, 'type');
  const kind = str(args, 'kind');
  const position = positionOf(args);
  const before = new Set(useAppStore.getState().rfNodes.map((n) => n.id));

  if (type === 'transport') {
    if (!(TRANSPORT_KINDS as readonly string[]).includes(kind)) {
      throw new Error(`未知传输类型 ${kind} (可选: ${TRANSPORT_KINDS.join('/')})`);
    }
    useAppStore.getState().addTransportNode(kind as TransportConfig['kind'], position);
    const created = useAppStore.getState().rfNodes.find((n) => !before.has(n.id));
    if (!created) throw new Error('节点创建失败');
    const patch = obj(args, 'config');
    if (Object.keys(patch).length > 0) {
      const data = created.data as unknown as TransportNodeData;
      useAppStore
        .getState()
        .setTransportNodeConfig(created.id, mergeTransportConfig(data.config, patch));
    }
    return { node_id: created.id };
  }

  if (type === 'protocol') {
    if (!(PROTOCOL_KINDS as readonly string[]).includes(kind)) {
      throw new Error(`未知协议类型 ${kind} (可选: ${PROTOCOL_KINDS.join('/')})`);
    }
    const config = buildProtocolConfig(kind, args.config);
    useAppStore.getState().addProtocolNode(config, position);
    const created = useAppStore.getState().rfNodes.find((n) => !before.has(n.id));
    if (!created) throw new Error('节点创建失败');
    return { node_id: created.id };
  }

  if (type === 'widget') {
    if (!(WIDGET_KINDS as readonly string[]).includes(kind)) {
      throw new Error(`未知控件类型 ${kind} (可选: ${WIDGET_KINDS.join('/')})`);
    }
    const tabId = optStr(args, 'tab_id') ?? useAppStore.getState().activeControlTabId;
    if (!useAppStore.getState().controlTabs.some((tab) => tab.id === tabId)) {
      throw new Error(`tab 不存在: ${tabId}`);
    }
    const widget = createWidget(kind as WidgetConfig['kind']);
    const patch = obj(args, 'config');
    if (Object.keys(patch).length > 0) {
      widget.params = deepMerge(widget.params, patch);
    }
    useAppStore.getState().addWidget(widget, tabId, position);
    return { node_id: widget.params.id };
  }

  throw new Error(`未知节点类别 ${type} (可选: transport/protocol/widget)`);
}

/** 更新节点配置 (widget 深合并 params; transport/protocol 更新 config)。 */
async function updateNodeConfig(args: Record<string, unknown>): Promise<unknown> {
  const nodeId = str(args, 'node_id');
  const patch = obj(args, 'config');
  if (Object.keys(patch).length === 0) throw new Error('缺少 config');
  const node = nodeOrThrow(nodeId);

  if (node.type === 'widget') {
    const widget = node.data.widget as WidgetConfig;
    const next = { ...widget, params: deepMerge(widget.params, patch) } as WidgetConfig;
    useAppStore.getState().updateWidget(nodeId, next);
    return { ok: true };
  }
  if (node.type === 'transport') {
    const data = node.data as unknown as TransportNodeData;
    useAppStore.getState().setTransportNodeConfig(nodeId, mergeTransportConfig(data.config, patch));
    return { ok: true };
  }
  if (node.type === 'protocol') {
    const data = node.data as unknown as ProtocolNodeData;
    const cur = data.config;
    const nextKind = typeof patch.kind === 'string' ? patch.kind : cur.kind;
    const config =
      nextKind !== cur.kind
        ? buildProtocolConfig(nextKind, patch)
        : buildProtocolConfig(cur.kind, { ...(cur as Record<string, unknown>), ...patch });
    await useAppStore.getState().setProtocolNodeConfig(nodeId, config);
    if ('convertTo' in patch || 'convert_to' in patch) {
      const cv = (patch.convertTo ?? patch.convert_to) as Record<string, unknown> | null;
      useAppStore
        .getState()
        .setProtocolNodeConvertTo(nodeId, cv ? buildProtocolConfig(str(cv, 'kind'), cv) : null);
    }
    return { ok: true };
  }
  throw new Error(`节点 ${nodeId} 类型不支持配置更新`);
}

/** 删除节点 (widget 或全局节点)。 */
function removeNode(args: Record<string, unknown>): unknown {
  const nodeId = str(args, 'node_id');
  const node = nodeOrThrow(nodeId);
  if (isGlobalNode(node)) {
    useAppStore.getState().removeGlobalNode(nodeId);
  } else if (node.type === 'widget') {
    useAppStore.getState().removeWidget(nodeId);
  } else {
    throw new Error(`节点 ${nodeId} 类型不支持删除`);
  }
  return { ok: true };
}

/** 连线 / 删线说明 — 拓扑工具已下沉后端权威 (cmd_graph::source_graph):
 *  connect_nodes / disconnect_edge 由内置 AI 后端直连执行, 编译失败
 *  (端口域不匹配/成环) 直接报错且不建边; 画布经 graph:source 事件收敛。
 *  此处不再注册前端 handler。 */

/** 移动节点。 */
function moveNode(args: Record<string, unknown>): unknown {
  const nodeId = str(args, 'node_id');
  const px = args.x;
  const py = args.y;
  if (typeof px !== 'number' || typeof py !== 'number') throw new Error('缺少 x/y 数值');
  nodeOrThrow(nodeId);
  useAppStore.getState().onNodesChange([
    { id: nodeId, type: 'position', position: { x: px, y: py }, dragging: false },
  ]);
  return { ok: true };
}

/** 新建控制页 (action 会把新页设为活跃)。 */
function createTab(args: Record<string, unknown>): unknown {
  const name = optStr(args, 'name');
  useAppStore.getState().addControlTab(name);
  return { tab_id: useAppStore.getState().activeControlTabId };
}

/** 连接 / 断开传输。 */
async function connectTransport(args: Record<string, unknown>): Promise<unknown> {
  const nodeId = str(args, 'node_id');
  nodeOrThrow(nodeId);
  await useAppStore.getState().connectNode(nodeId);
  return { node_id: nodeId, state: useAppStore.getState().connectionStates[nodeId] ?? 'Disconnected' };
}

async function disconnectTransport(args: Record<string, unknown>): Promise<unknown> {
  const nodeId = str(args, 'node_id');
  nodeOrThrow(nodeId);
  await useAppStore.getState().disconnectNode(nodeId);
  return { ok: true };
}

/** 模板清单 (含本地化名称)。 */
function listTemplates(): unknown {
  const lang = useAppStore.getState().lang;
  return {
    templates: QUICK_START_TEMPLATES.map((tpl) => ({
      id: tpl.id,
      name: t(lang, tpl.nameKey as never),
      description: t(lang, tpl.descKey as never),
    })),
  };
}

/** 应用模板 — AI 默认 merge (追加新页, 不破坏用户现有工作区)。 */
async function applyTemplateTool(args: Record<string, unknown>): Promise<unknown> {
  const templateId = str(args, 'template_id');
  const tpl = getTemplate(templateId);
  if (!tpl) {
    throw new Error(`模板不存在: ${templateId} (先 list_templates 查询)`);
  }
  const mode: TemplateApplyMode = args.mode === 'replace' ? 'replace' : 'merge';
  await applyTemplate(tpl.build(), mode);
  return { ok: true, mode };
}

// ============ 注册与分发 ============

const HANDLERS: Record<string, ToolHandler> = {
  get_workspace: () => getWorkspace(),
  add_node: (args) => addNode(args),
  update_node_config: (args) => updateNodeConfig(args),
  remove_node: (args) => removeNode(args),
  move_node: (args) => moveNode(args),
  create_tab: (args) => createTab(args),
  set_active_tab: (args) => {
    const tabId = str(args, 'tab_id');
    if (!useAppStore.getState().controlTabs.some((tab) => tab.id === tabId)) {
      throw new Error(`tab 不存在: ${tabId}`);
    }
    useAppStore.getState().setActiveControlTab(tabId);
    return { ok: true };
  },
  connect_transport: (args) => connectTransport(args),
  disconnect_transport: (args) => disconnectTransport(args),
  list_templates: () => listTemplates(),
  apply_template: (args) => applyTemplateTool(args),
};

let started = false;

/** 初始化工具宿主 — App 挂载时调用一次 (重复调用幂等)。 */
export function initAiToolHost(): void {
  if (started) return;
  started = true;
  void listen<AiToolInvoke>('ai_tool_invoke', (event) => {
    const { call_id, name, arguments: args } = event.payload;
    void (async () => {
      const handler = HANDLERS[name];
      try {
        if (!handler) throw new Error(`前端未实现工具: ${name}`);
        const result = await handler(args ?? {});
        await invoke('ai_tool_resolve', {
          callId: call_id,
          ok: true,
          result: result ?? { ok: true },
        });
      } catch (e) {
        await invoke('ai_tool_resolve', {
          callId: call_id,
          ok: false,
          result: e instanceof Error ? e.message : String(e),
        });
      }
    })();
  });
}
