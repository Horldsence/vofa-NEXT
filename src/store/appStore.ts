import { create } from 'zustand';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  applyNodeChanges,
  applyEdgeChanges,
  addEdge,
  type Node,
  type Edge,
  type NodeChange,
  type EdgeChange,
  type Connection,
  MarkerType,
} from '@xyflow/react';
import { api } from '../lib/tauri';
import { waveformWindow, rawDataBuffer } from '../lib/dataBuffer';
import { notify, formatError } from '../lib/notifications';
import { nanoid } from 'nanoid';
import { t } from '../i18n';
import type { Lang } from '../i18n';
import type {
  ConnectionState,
  DataFrame,
  PortInfo,
  ProtocolConfig,
  RawData,
  TransportConfig,
  TransportStats,
  WidgetConfig,
  WidgetBinding,
  NodeGraphEdge,
  ControlTab,
  DataTab,
} from '../types';
import {
  computeAllOutputs,
  type WidgetOutputCache,
} from '../lib/widgetDataFlow';

const DEFAULT_SERIAL: TransportConfig = {
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

/// 默认协议: JustFloat 自动检测通道
const DEFAULT_PROTOCOL: ProtocolConfig = {
  kind: 'JustFloat',
  channels: null, // null = 自动检测
};

/// 通道源节点 ID (全局唯一, 不可删除)
export const CHANNEL_SOURCE_ID = '__channel_source__';

/// 侧边栏视图类型 — 顺序符合配置操作流: 接口 → 协议 → 串口
export type SidebarView =
  | 'transport'
  | 'protocol'
  | 'port'
  | 'widgets'
  | 'ai';

interface AppStore {
  // Language
  lang: Lang;
  setLang: (lang: Lang) => void;

  // Sidebar navigation
  sidebarView: SidebarView;
  sidebarVisible: boolean;
  setSidebarView: (view: SidebarView) => void;
  toggleSidebar: (view: SidebarView) => void;

  // Connection
  connectionState: ConnectionState;
  stats: TransportStats;

  // Config
  transportConfig: TransportConfig;
  protocolConfig: ProtocolConfig;
  ports: PortInfo[];
  selectedPortIndex: number;

  /// 自动检测到的通道数 (仅在自动模式下有值)
  detectedChannels: number | null;

  // Widgets
  widgets: WidgetConfig[];

  // Control tabs
  controlTabs: ControlTab[];
  activeControlTabId: string;

  // React Flow node graph (global, filtered by tabId when rendering)
  rfNodes: Node[];
  rfEdges: Edge[];

  // Data tabs
  dataTabs: DataTab[];
  activeDataTabId: string;

  // Raw data version (for triggering re-renders)
  rawDataVersion: number;

  // Widget 数据流缓存: widgetId -> portId -> value
  // 由 polling loop 定时更新 (见 useWidgetDataFlow hook)
  // 上游 widget 的输出值, 供下游 widget 读取
  widgetOutputCache: WidgetOutputCache;
  setWidgetOutput: (widgetId: string, portId: string, value: number) => void;
  refreshWidgetOutputCache: () => void;

  // Actions
  refreshPorts: () => Promise<void>;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  setTransportConfig: (config: TransportConfig) => void;
  setProtocolConfig: (config: ProtocolConfig) => Promise<void>;
  sendData: (data: number[]) => Promise<void>;
  sendText: (text: string) => Promise<void>;
  sendWidgetValue: (binding: WidgetBinding, value: number) => Promise<void>;
  selectPort: (index: number) => void;
  pollDetectedChannels: () => Promise<void>;

  // Widget management
  addWidget: (widget: WidgetConfig, tabId: string, position?: { x: number; y: number }) => void;
  removeWidget: (id: string) => void;
  updateWidget: (id: string, widget: WidgetConfig) => void;

  // Custom widget editor
  customEditorState: { open: boolean; widgetId: string | null };
  openCustomEditor: (widgetId?: string) => void;
  closeCustomEditor: () => void;

  // Control tabs
  addControlTab: (name?: string) => void;
  removeControlTab: (tabId: string) => void;
  setActiveControlTab: (tabId: string) => void;
  renameControlTab: (tabId: string, name: string) => void;

  // React Flow
  onNodesChange: (changes: NodeChange[]) => void;
  onEdgesChange: (changes: EdgeChange[]) => void;
  onConnect: (connection: Connection) => void;
  /// 获取指定 tab 的节点和边 (过滤)
  getTabNodes: (tabId: string) => Node[];
  getTabEdges: (tabId: string) => Edge[];

  // Data tabs
  addDataTab: (tab: DataTab) => void;
  removeDataTab: (tabId: string) => void;
  setActiveDataTab: (tabId: string) => void;

  // Data
  clearData: () => Promise<void>;

  // Event setup
  initEventListeners: () => Promise<() => void>;
}

let unlistenFns: UnlistenFn[] = [];
let waveformSub: { cancel: () => void } | null = null;
let detectedChannelsPoller: ReturnType<typeof setInterval> | null = null;
let flushTimer: ReturnType<typeof setTimeout> | null = null;

/// 创建通道源节点 (每个 tab 一个)
function createChannelSourceNode(tabId: string, channelCount: number): Node {
  return {
    id: `${CHANNEL_SOURCE_ID}-${tabId}`,
    type: 'channelSource',
    position: { x: 40, y: 40 },
    data: { tabId, channelCount, label: 'Channel Source' },
    selectable: false,
    deletable: false,
  };
}

/// 同步节点图到后端
async function syncNodeGraphToBackend(edges: Edge[]): Promise<void> {
  const graphEdges: NodeGraphEdge[] = edges
    .filter((e) => !e.source.startsWith(CHANNEL_SOURCE_ID) || !e.target.startsWith(CHANNEL_SOURCE_ID))
    .map((e) => ({
      id: e.id,
      source: e.source,
      source_handle: e.sourceHandle ?? '',
      target: e.target,
      target_handle: e.targetHandle ?? '',
    }));
  try {
    await api.updateNodeGraph(graphEdges);
  } catch (err) {
    const lang = useAppStore.getState().lang;
    notify.error(
      t(lang, 'notifNodeGraphSyncFailed'),
      formatError(err),
      { source: 'syncNodeGraph' }
    );
  }
}

/// 获取当前生效通道数 (优先检测值, 其次配置值)
function getEffectiveChannels(
  protocolConfig: ProtocolConfig,
  detectedChannels: number | null
): number {
  if (protocolConfig.kind === 'RawData') return 4;
  const configured = protocolConfig.channels;
  if (configured != null) return configured;
  return detectedChannels ?? 4;
}

export const useAppStore = create<AppStore>((set, get) => ({
  lang: 'zh',
  setLang: (lang) => set({ lang }),

  // 侧边栏导航 — 默认从数据接口开始配置
  sidebarView: 'transport',
  sidebarVisible: true,
  setSidebarView: (view) => set({ sidebarView: view, sidebarVisible: true }),
  toggleSidebar: (view) => {
    const { sidebarView, sidebarVisible } = get();
    if (sidebarView === view && sidebarVisible) {
      set({ sidebarVisible: false });
    } else {
      set({ sidebarView: view, sidebarVisible: true });
    }
  },

  connectionState: 'Disconnected',
  stats: { rx_bytes: 0, tx_bytes: 0, rx_frames: 0, tx_frames: 0 },

  transportConfig: DEFAULT_SERIAL,
  protocolConfig: DEFAULT_PROTOCOL,
  ports: [],
  selectedPortIndex: 0,
  detectedChannels: null,

  widgets: [],

  customEditorState: { open: false, widgetId: null },
  openCustomEditor: (widgetId) =>
    set((s) => {
      // 未指定 widgetId → 创建新 Custom widget 并打开编辑器
      if (!widgetId) {
        const widget = createWidget('Custom');
        const tabId = s.activeControlTabId;
        const pos = { x: 280, y: 80 + Math.random() * 100 };
        const newNode: Node = {
          id: widget.params.id,
          type: 'widget',
          position: pos,
          data: { widget, tabId },
        };
        return {
          widgets: [...s.widgets, widget],
          rfNodes: [...s.rfNodes, newNode],
          controlTabs: s.controlTabs.map((t) =>
            t.id === tabId ? { ...t, widgets: [...t.widgets, widget.params.id] } : t
          ),
          customEditorState: { open: true, widgetId: widget.params.id },
        };
      }
      return { customEditorState: { open: true, widgetId } };
    }),
  closeCustomEditor: () => set({ customEditorState: { open: false, widgetId: null } }),

  controlTabs: [{ id: 'default', name: 'Tab 1', widgets: [] }],
  activeControlTabId: 'default',

  // 初始化: 为每个 tab 创建一个通道源节点
  rfNodes: [createChannelSourceNode('default', 4)],
  rfEdges: [],

  dataTabs: [
    { id: 'waveform-fixed', type: 'waveform' as const, name: 'Waveform', closable: false },
    { id: 'raw-fixed', type: 'raw' as const, name: 'Raw Data', closable: false },
  ],
  activeDataTabId: 'waveform-fixed',

  rawDataVersion: 0,

  // Widget 数据流缓存: widgetId -> portId -> value
  // 由 polling loop 定时更新 (computeAllOutputs 调用)
  widgetOutputCache: {},
  setWidgetOutput: (widgetId, portId, value) =>
    set((s) => {
      const cur = s.widgetOutputCache[widgetId] ?? {};
      // 浅比较避免无谓的更新
      if (cur[portId] === value) return {};
      return {
        widgetOutputCache: {
          ...s.widgetOutputCache,
          [widgetId]: { ...cur, [portId]: value },
        },
      };
    }),
  refreshWidgetOutputCache: () =>
    set((s) => {
      const next = computeAllOutputs(s.widgets, s.rfEdges, s.widgetOutputCache);
      // 浅比较: 引用相同则不更新
      if (next === s.widgetOutputCache) return {};
      return { widgetOutputCache: next };
    }),

  refreshPorts: async () => {
    try {
      const ports = await api.listPorts();
      set({ ports });
    } catch (e) {
      const lang = get().lang;
      notify.error(
        t(lang, 'notifRefreshPortsFailed'),
        formatError(e),
        {
          source: 'refreshPorts',
          actions: [{ label: t(lang, 'notifRetry'), run: () => { void get().refreshPorts(); } }],
        }
      );
    }
  },

  connect: async () => {
    const { transportConfig, protocolConfig } = get();
    try {
      await api.setProtocol(protocolConfig);
      await api.clearBuffer();
      rawDataBuffer.clear();
      waveformWindow.clear();
      await api.openTransport(transportConfig);
      set({
        connectionState: 'Connected',
        stats: { rx_bytes: 0, tx_bytes: 0, rx_frames: 0, tx_frames: 0 },
        rawDataVersion: Date.now(),
      });
      // 启动波形订阅
      if (waveformSub) {
        waveformSub.cancel();
      }
      waveformSub = api.subscribeWaveform(
        (window) => waveformWindow.set(window),
        { intervalMs: 33, maxPoints: 2000 }
      );
      // 启动自动通道检测轮询 (仅在自动模式下)
      get().pollDetectedChannels();
    } catch (e) {
      const lang = get().lang;
      set({ connectionState: 'Error' });
      notify.error(
        t(lang, 'notifConnectFailed'),
        formatError(e),
        {
          source: 'connect',
          actions: [{ label: t(lang, 'notifRetry'), run: () => { void get().connect(); } }],
        }
      );
    }
  },

  disconnect: async () => {
    try {
      await api.closeTransport();
      if (waveformSub) {
        waveformSub.cancel();
        waveformSub = null;
      }
      if (detectedChannelsPoller) {
        clearInterval(detectedChannelsPoller);
        detectedChannelsPoller = null;
      }
      set({ connectionState: 'Disconnected' });
    } catch (e) {
      const lang = get().lang;
      notify.error(
        t(lang, 'notifDisconnectFailed'),
        formatError(e),
        {
          source: 'disconnect',
          actions: [{ label: t(lang, 'notifRetry'), run: () => { void get().disconnect(); } }],
        }
      );
    }
  },

  setTransportConfig: (config) => set({ transportConfig: config }),

  setProtocolConfig: async (config) => {
    set({ protocolConfig: config });
    try {
      await api.setProtocol(config);
      // 手动模式: 设置后端缓冲区通道数; 自动模式: 由后端动态扩展
      if (config.kind !== 'RawData' && config.channels != null) {
        await api.setBufferChannels(config.channels);
        set({ detectedChannels: null });
      }
    } catch (e) {
      const lang = get().lang;
      notify.error(
        t(lang, 'notifSetProtocolFailed'),
        formatError(e),
        { source: 'setProtocol' }
      );
    }
  },

  sendData: async (data) => {
    try {
      await api.sendRaw(data);
    } catch (e) {
      const lang = get().lang;
      notify.error(
        t(lang, 'notifSendFailed'),
        formatError(e),
        { source: 'sendData' }
      );
    }
  },

  sendText: async (text) => {
    try {
      await api.sendString(text);
    } catch (e) {
      const lang = get().lang;
      notify.error(
        t(lang, 'notifSendFailed'),
        formatError(e),
        { source: 'sendText' }
      );
    }
  },

  sendWidgetValue: async (binding, value) => {
    try {
      await api.sendWidgetValue(binding, value);
    } catch (e) {
      const lang = get().lang;
      notify.error(
        t(lang, 'notifSendFailed'),
        formatError(e),
        { source: 'sendWidget' }
      );
    }
  },

  selectPort: (index) => {
    const { ports, transportConfig } = get();
    if (index >= 0 && index < ports.length) {
      if (transportConfig.kind === 'Serial') {
        set({
          selectedPortIndex: index,
          transportConfig: {
            ...transportConfig,
            params: {
              ...transportConfig.params,
              port_name: ports[index].name,
            },
          },
        });
      } else {
        set({ selectedPortIndex: index });
      }
    }
  },

  /// 轮询自动检测到的通道数 (仅在自动模式下查询)
  pollDetectedChannels: async () => {
    const { protocolConfig } = get();
    if (protocolConfig.kind === 'RawData') return;
    if (protocolConfig.channels != null) return; // 手动模式不轮询

    if (detectedChannelsPoller) clearInterval(detectedChannelsPoller);
    detectedChannelsPoller = setInterval(async () => {
      try {
        const detected = await api.getDetectedChannels();
        const prev = get().detectedChannels;
        if (detected !== prev) {
          set({ detectedChannels: detected });
          // 更新通道源节点的 channelCount
          const effective = getEffectiveChannels(protocolConfig, detected);
          set((s) => ({
            rfNodes: s.rfNodes.map((n) =>
              n.type === 'channelSource' && n.data.tabId
                ? { ...n, data: { ...n.data, channelCount: effective } }
                : n
            ),
          }));
        }
      } catch (e) {
        const lang = get().lang;
        notify.warn(
          t(lang, 'notifPollChannelsFailed'),
          formatError(e),
          { source: 'pollChannels' }
        );
      }
    }, 1000);
  },

  addWidget: (widget, tabId, position) =>
    set((s) => {
      const pos = position ?? { x: 240 + Math.random() * 100, y: 80 + Math.random() * 80 };
      const newNode: Node = {
        id: widget.params.id,
        type: 'widget',
        position: pos,
        data: { widget, tabId },
      };
      const newState: Partial<AppStore> = {
        widgets: [...s.widgets, widget],
        rfNodes: [...s.rfNodes, newNode],
      };
      // 显示控件自动创建 DataTab
      if (
        widget.kind === 'Waveform' ||
        widget.kind === 'PieChart' ||
        widget.kind === 'Image'
      ) {
        const tabType =
          widget.kind === 'Waveform'
            ? 'waveform-extra'
            : widget.kind === 'PieChart'
            ? 'pie'
            : 'image';
        const tabName =
          widget.kind === 'Waveform'
            ? 'Waveform'
            : widget.params.label;
        const newTab: DataTab = {
          id: widget.params.id,
          type: tabType as DataTab['type'],
          name: tabName,
          widgetId: widget.params.id,
          closable: true,
        };
        newState.dataTabs = [...s.dataTabs, newTab];
        newState.activeDataTabId = widget.params.id;
      }
      // 加入 Tab
      newState.controlTabs = s.controlTabs.map((t) =>
        t.id === tabId ? { ...t, widgets: [...t.widgets, widget.params.id] } : t
      );
      return newState;
    }),

  removeWidget: (id) =>
    set((s) => {
      const widget = s.widgets.find((w) => w.params.id === id);
      const newState: Partial<AppStore> = {
        widgets: s.widgets.filter((w) => w.params.id !== id),
        rfNodes: s.rfNodes.filter((n) => n.id !== id),
        rfEdges: s.rfEdges.filter((e) => e.source !== id && e.target !== id),
      };
      // 移除关联的 DataTab
      if (
        widget &&
        (widget.kind === 'Waveform' ||
          widget.kind === 'PieChart' ||
          widget.kind === 'Image')
      ) {
        const remaining = s.dataTabs.filter((t) => t.id !== id);
        newState.dataTabs = remaining;
        if (s.activeDataTabId === id) {
          newState.activeDataTabId = remaining[0]?.id ?? 'waveform-fixed';
        }
      }
      // 从所有 controlTabs 中移除
      newState.controlTabs = s.controlTabs.map((t) => ({
        ...t,
        widgets: t.widgets.filter((w) => w !== id),
      }));
      return newState;
    }),

  updateWidget: (id, widget) =>
    set((s) => ({
      widgets: s.widgets.map((w) => (w.params.id === id ? widget : w)),
      rfNodes: s.rfNodes.map((n) =>
        n.id === id ? { ...n, data: { ...n.data, widget } } : n
      ),
    })),

  addControlTab: (name) =>
    set((s) => {
      const id = nanoid(8);
      const tabName = name || `Tab ${s.controlTabs.length + 1}`;
      const effectiveCh = getEffectiveChannels(s.protocolConfig, s.detectedChannels);
      return {
        controlTabs: [...s.controlTabs, { id, name: tabName, widgets: [] }],
        activeControlTabId: id,
        rfNodes: [...s.rfNodes, createChannelSourceNode(id, effectiveCh)],
      };
    }),

  removeControlTab: (tabId) =>
    set((s) => {
      const remaining = s.controlTabs.filter((t) => t.id !== tabId);
      if (remaining.length === 0) {
        const defaultTab = { id: 'default', name: 'Tab 1', widgets: [] };
        return {
          controlTabs: [defaultTab],
          activeControlTabId: 'default',
        };
      }
      // 移除该 tab 的所有节点和边
      const tabNodeIds = new Set(
        s.rfNodes.filter((n) => n.data.tabId === tabId).map((n) => n.id)
      );
      tabNodeIds.add(`${CHANNEL_SOURCE_ID}-${tabId}`);
      return {
        controlTabs: remaining,
        activeControlTabId:
          s.activeControlTabId === tabId ? remaining[0].id : s.activeControlTabId,
        rfNodes: s.rfNodes.filter((n) => n.data.tabId !== tabId && n.id !== `${CHANNEL_SOURCE_ID}-${tabId}`),
        rfEdges: s.rfEdges.filter((e) => !tabNodeIds.has(e.source) && !tabNodeIds.has(e.target)),
      };
    }),

  setActiveControlTab: (tabId) => set({ activeControlTabId: tabId }),

  renameControlTab: (tabId, name) =>
    set((s) => ({
      controlTabs: s.controlTabs.map((t) =>
        t.id === tabId ? { ...t, name } : t
      ),
    })),

  onNodesChange: (changes) => {
    set((s) => ({
      rfNodes: applyNodeChanges(changes, s.rfNodes),
    }));
  },

  onEdgesChange: (changes) => {
    set((s) => {
      const newEdges = applyEdgeChanges(changes, s.rfEdges);
      // 异步同步到后端 (不阻塞 UI)
      void syncNodeGraphToBackend(newEdges);
      return { rfEdges: newEdges };
    });
  },

  onConnect: (connection) => {
    set((s) => {
      const newEdge: Edge = {
        ...connection,
        id: nanoid(8),
        animated: true,
        markerEnd: { type: MarkerType.ArrowClosed, width: 12, height: 12 },
      };
      const newEdges = addEdge(newEdge, s.rfEdges);
      void syncNodeGraphToBackend(newEdges);
      return { rfEdges: newEdges };
    });
  },

  getTabNodes: (tabId) => {
    const { rfNodes } = get();
    return rfNodes.filter(
      (n) => n.data.tabId === tabId || (n.type === 'channelSource' && n.id === `${CHANNEL_SOURCE_ID}-${tabId}`)
    );
  },

  getTabEdges: (tabId) => {
    const { rfNodes, rfEdges } = get();
    const tabNodeIds = new Set(
      rfNodes
        .filter((n) => n.data.tabId === tabId || (n.type === 'channelSource' && n.id === `${CHANNEL_SOURCE_ID}-${tabId}`))
        .map((n) => n.id)
    );
    return rfEdges.filter((e) => tabNodeIds.has(e.source) && tabNodeIds.has(e.target));
  },

  addDataTab: (tab) =>
    set((s) => ({
      dataTabs: [...s.dataTabs, tab],
      activeDataTabId: tab.id,
    })),

  removeDataTab: (tabId) =>
    set((s) => {
      const tab = s.dataTabs.find((t) => t.id === tabId);
      if (!tab || !tab.closable) return s;
      const remaining = s.dataTabs.filter((t) => t.id !== tabId);
      return {
        dataTabs: remaining,
        activeDataTabId:
          s.activeDataTabId === tabId ? remaining[0]?.id ?? 'waveform-fixed' : s.activeDataTabId,
      };
    }),

  setActiveDataTab: (tabId) => set({ activeDataTabId: tabId }),

  clearData: async () => {
    try {
      await api.clearBuffer();
    } catch (e) {
      const lang = get().lang;
      notify.error(
        t(lang, 'notifClearBufferFailed'),
        formatError(e),
        { source: 'clearBuffer' }
      );
    }
    rawDataBuffer.clear();
    waveformWindow.clear();
    set({ rawDataVersion: Date.now() });
  },

  initEventListeners: async () => {
    // Clean up previous listeners
    unlistenFns.forEach((fn) => fn());
    unlistenFns = [];

    const unlistenData = await listen<RawData>('transport:data', (event) => {
      const { data } = event.payload;
      rawDataBuffer.push(data);
      // Throttle raw data display updates
      if (flushTimer) return;
      flushTimer = setTimeout(() => {
        rawDataBuffer.notify();
        flushTimer = null;
      }, 50);
    });

    // transport:frames 事件 (批量帧, 后端重构后一次 emit 多帧)
    const unlistenFrames = await listen<DataFrame[]>('transport:frames', () => {
      // 后端已将帧推入 DataBuffer, 通过 subscribe_waveform Channel 推送窗口
      // 此处无需前端再缓冲
    });

    const unlistenState = await listen<ConnectionState>('transport:state', (event) => {
      set({ connectionState: event.payload });
    });

    const unlistenStats = await listen<TransportStats>('transport:rx', (event) => {
      set((s) => ({
        stats: {
          rx_bytes: s.stats.rx_bytes + event.payload.rx_bytes,
          tx_bytes: s.stats.tx_bytes + event.payload.tx_bytes,
          rx_frames: s.stats.rx_frames + event.payload.rx_frames,
          tx_frames: s.stats.tx_frames + event.payload.tx_frames,
        },
      }));
    });

    unlistenFns = [unlistenData, unlistenFrames, unlistenState, unlistenStats];

    return () => {
      unlistenFns.forEach((fn) => fn());
      unlistenFns = [];
      if (waveformSub) {
        waveformSub.cancel();
        waveformSub = null;
      }
      if (detectedChannelsPoller) {
        clearInterval(detectedChannelsPoller);
        detectedChannelsPoller = null;
      }
    };
  },
}));

/// Custom widget 编辑器默认代码 (与 CustomWidgetEditor 中常量保持一致)
const DEFAULT_CUSTOM_CODE = `({
  name: 'MyWidget',
  description: '自定义控件示例',
  inputs: [
    { id: 'value', label: 'Value' }
  ],
  outputs: [],
  settings: [
    { id: 'unit', label: 'Unit', type: 'text', default: 'V' },
    { id: 'color', label: 'Color', type: 'color', default: '#75beff' }
  ],
  onMount: function(ctx) {
    ctx.state.count = 0;
  },
  render: function(ctx) {
    const v = ctx.inputs.value ?? 0;
    const u = ctx.settings.unit || '';
    const c = ctx.settings.color || '#75beff';
    ctx.el.innerHTML =
      '<div style="padding:8px;text-align:center;font-family:sans-serif">' +
        '<div style="font-size:24px;color:' + c + ';font-weight:bold">' +
          Number(v).toFixed(2) +
        '</div>' +
        '<div style="font-size:11px;color:#888">' + u + '</div>' +
      '</div>';
  }
})
`;

/// 辅助函数: 创建控件
export function createWidget(kind: WidgetConfig['kind']): WidgetConfig {
  const id = nanoid(8);
  switch (kind) {
    case 'Knob':
      return {
        kind: 'Knob',
        params: {
          id, label: 'Knob', min: 0, max: 100, step: 1, default: 50,
          binding: { mode: 'None' },
        },
      };
    case 'Button':
      return {
        kind: 'Button',
        params: {
          id, label: 'Button', press_value: 1, release_value: 0,
          binding: { mode: 'None' },
        },
      };
    case 'Radio':
      return {
        kind: 'Radio',
        params: {
          id, label: 'Radio', options: [['A', 0], ['B', 1]], default: 0,
          binding: { mode: 'None' },
        },
      };
    case 'Checkbox':
      return {
        kind: 'Checkbox',
        params: {
          id, label: 'Checkbox', checked_value: 1, unchecked_value: 0, default: false,
          binding: { mode: 'None' },
        },
      };
    case 'Slider':
      return {
        kind: 'Slider',
        params: {
          id, label: 'Slider', min: 0, max: 100, step: 1, default: 50,
          binding: { mode: 'None' },
        },
      };
    case 'Label':
      return {
        kind: 'Label',
        params: { id, text: 'Label', channel: null },
      };
    case 'Waveform':
      return {
        kind: 'Waveform',
        params: { id, channels: 4, max_points: 10000, visible_channels: [true, true, true, true] },
      };
    case 'PieChart':
      return {
        kind: 'PieChart',
        params: { id, label: 'Pie', segments: ['A', 'B', 'C'], channels: [0, 1, 2] },
      };
    case 'Image':
      return {
        kind: 'Image',
        params: { id, label: 'Image', width: 320, height: 240, format: 'rgb888' },
      };
    case 'Gauge':
      return {
        kind: 'Gauge',
        params: { id, label: 'Gauge', min: 0, max: 100, unit: '', channel: null },
      };
    case 'LED':
      return {
        kind: 'LED',
        params: {
          id, label: 'LED', threshold: 0.5,
          on_color: '#89d185', off_color: '#3c3c3c', channel: null,
        },
      };
    case 'NumberDisplay':
      return {
        kind: 'NumberDisplay',
        params: { id, label: 'Value', unit: '', precision: 2, channel: null },
      };
    case 'Custom':
      return {
        kind: 'Custom',
        params: { id, label: 'Custom', code: DEFAULT_CUSTOM_CODE, settings: {} },
      };
    case 'Math':
      return {
        kind: 'Math',
        params: {
          id,
          label: 'Math',
          op: 'add',
          inputCount: 2,
          unit: '',
          precision: 3,
        },
      };
  }
}

