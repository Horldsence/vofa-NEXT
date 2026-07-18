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
import {
  subscribeGraphOutputs,
  subscribeCustomInputs,
  subscribeSpectrum,
  setInputValue as apiSetInputValue,
  submitCustomOutput as apiSubmitCustomOutput,
} from '../lib/graphSubscription';
import { canFrameBuffer } from '../lib/canBuffer';
import { subscribeCanFrames } from '../lib/canSubscription';
import { logicSampleBuffer, decodedEventBuffer } from '../lib/logicBuffer';
import { subscribeLogicSamples, subscribeDecodedEvents } from '../lib/logicSubscription';
import { widgetToNodeKind, makeChannelSourceNodeDef, edgeToGraphEdge, type NodeDef } from '../lib/nodeDef';
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
  ControlTab,
  DataTab,
  SpectrumResult,
  WindowType,
  SpectrumOutput,
  CanFrame,
  LogicSample,
  DecodedEvent,
} from '../types';

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

/// 侧边栏视图类型 — 顺序符合配置操作流: 数据接口 → 协议引擎 → 控件
export type SidebarView =
  | 'transport'
  | 'protocol'
  | 'widgets';

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

  // ===== 后端图评估状态 (60 FPS / 30 FPS 推送) =====
  /// 后端图输出快照: widgetId -> portId -> value
  /// 由 subscribeGraphOutputs 推送 (60 FPS), 供显示控件读取
  graphOutputs: Record<string, Record<string, number>>;
  graphOutputsTick: number;
  /// Custom widget 输入批次: widgetId -> portId -> value
  /// 由 subscribeCustomInputs 推送 (30 FPS), 供 Custom iframe 读取
  customInputs: Record<string, Record<string, number>>;
  /// 频谱分析结果: sinkWidgetId -> SpectrumResult
  /// 由 subscribeSpectrum 推送 (30 FPS), 供 SpectrumChart 读取
  spectrumResults: Record<string, SpectrumResult>;

  // CAN 帧相关
  canFrames: CanFrame[];
  canFramesVersion: number;
  addCanTab: () => void;

  // 逻辑分析仪相关
  logicSamples: LogicSample[];
  decodedEvents: DecodedEvent[];
  logicSamplesVersion: number;
  addLogicTab: () => void;

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

  // Test data
  testDataRunning: boolean;
  startTestData: () => Promise<void>;
  stopTestData: () => Promise<void>;

  // Widget management
  addWidget: (widget: WidgetConfig, tabId: string, position?: { x: number; y: number }) => void;
  removeWidget: (id: string) => void;
  updateWidget: (id: string, widget: WidgetConfig) => void;

  // ===== 图同步 actions (后端评估) =====
  /// 同步指定 tab 的图到后端 (整体替换 nodes + edges)
  syncTabGraph: (tabId: string) => void;
  /// 移除指定 tab 的图 (tab 删除时调用)
  removeTabGraph: (tabId: string) => void;
  /// 输入控件值变更 → invoke 后端 (事件驱动)
  setInputValue: (widgetId: string, value: number) => void;
  /// Custom widget 输出回传 → invoke 后端
  submitCustomOutput: (widgetId: string, outputs: Record<string, number>) => void;

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
let graphOutputSub: { cancel: () => void } | null = null;
let customInputSub: { cancel: () => void } | null = null;
let spectrumSub: { cancel: () => void } | null = null;
let canFramesSub: { cancel: () => void } | null = null;
let logicSamplesSub: { cancel: () => void } | null = null;
let decodedEventsSub: { cancel: () => void } | null = null;
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

/// 同步指定 tab 的节点图到后端
/// 收集该 tab 的所有节点 (ChannelSource + widgets) 与边, 整体替换后端图
async function syncTabGraphToBackend(tabId: string): Promise<void> {
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
function getEffectiveChannels(
  protocolConfig: ProtocolConfig,
  detectedChannels: number | null
): number {
  if (protocolConfig.kind === 'RawData' || protocolConfig.kind === 'Slcan' || protocolConfig.kind === 'CandleLight' || protocolConfig.kind === 'LogicDecode') return 4;
  const configured = protocolConfig.channels;
  if (configured != null) return configured;
  return detectedChannels ?? 4;
}

export const useAppStore = create<AppStore>((set, get) => ({
  lang: 'zh',
  setLang: (lang) => set({ lang }),

  // 侧边栏导航 — 默认从数据接口开始
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
  /// -1 表示未选中任何端口 (避免 Windows 下默认高亮首项但不生效的问题)
  selectedPortIndex: -1,
  detectedChannels: null,

  testDataRunning: false,

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

  // 后端图评估状态 — 由 initEventListeners 中的 subscribeGraphOutputs / subscribeCustomInputs / subscribeSpectrum 推送
  graphOutputs: {},
  graphOutputsTick: 0,
  customInputs: {},
  spectrumResults: {},

  canFrames: [],
  canFramesVersion: 0,

  addCanTab: () => {
    const existing = get().dataTabs.find((t) => t.type === 'can');
    if (existing) {
      set({ activeDataTabId: existing.id });
      return;
    }
    const tab: DataTab = {
      id: `can-${Date.now()}`,
      type: 'can',
      name: t(get().lang, 'canFrames'),
      closable: true,
    };
    set({
      dataTabs: [...get().dataTabs, tab],
      activeDataTabId: tab.id,
    });
  },

  logicSamples: [],
  decodedEvents: [],
  logicSamplesVersion: 0,

  addLogicTab: () => {
    const existing = get().dataTabs.find((t) => t.type === 'logic');
    if (existing) {
      set({ activeDataTabId: existing.id });
      return;
    }
    const tab: DataTab = {
      id: `logic-${Date.now()}`,
      type: 'logic',
      name: t(get().lang, 'logicAnalyzer'),
      closable: true,
    };
    set({
      dataTabs: [...get().dataTabs, tab],
      activeDataTabId: tab.id,
    });
  },

  // 图同步 actions
  syncTabGraph: (tabId) => {
    void syncTabGraphToBackend(tabId);
  },
  removeTabGraph: (tabId) => {
    void api.removeTabGraph(tabId);
  },
  setInputValue: (widgetId, value) => {
    void apiSetInputValue(widgetId, value);
  },
  submitCustomOutput: (widgetId, outputs) => {
    void apiSubmitCustomOutput(widgetId, outputs);
  },

  refreshPorts: async () => {
    try {
      const ports = await api.listPorts();
      // 刷新端口列表后, 按 port_name 保留选中状态 (而非依赖 index)
      const { transportConfig, selectedPortIndex } = get();
      const isSerialLike = transportConfig.kind === 'Serial' || transportConfig.kind === 'Slcan';
      const currentName = isSerialLike
        ? (transportConfig.params as { port_name: string }).port_name
        : '';
      let newIndex = -1;
      if (currentName) {
        newIndex = ports.findIndex((p) => p.name === currentName);
      } else if (selectedPortIndex >= 0 && selectedPortIndex < ports.length) {
        // 兜底: 旧 index 仍有效则保留
        newIndex = selectedPortIndex;
      }
      set({ ports, selectedPortIndex: newIndex });
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
        sidebarView: 'protocol',
        sidebarVisible: true,
        testDataRunning: false,
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
      set({ connectionState: 'Disconnected', testDataRunning: false });
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

  startTestData: async () => {
    try {
      await api.startTestData();
      set({ testDataRunning: true });
    } catch (e) {
      const lang = get().lang;
      notify.error(t(lang, 'notifStartTestDataFailed'), formatError(e), { source: 'startTestData' });
    }
  },

  stopTestData: async () => {
    try {
      await api.stopTestData();
      set({ testDataRunning: false });
    } catch (e) {
      const lang = get().lang;
      notify.error(t(lang, 'notifStopTestDataFailed'), formatError(e), { source: 'stopTestData' });
    }
  },

  setProtocolConfig: async (config) => {
    set({ protocolConfig: config });
    try {
      await api.setProtocol(config);
      // 手动模式: 设置后端缓冲区通道数; 自动模式: 由后端动态扩展
      if ((config.kind === 'JustFloat' || config.kind === 'FireWater') && config.channels != null) {
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
      // Serial 与 Slcan 都基于 USB-CDC 串口, 选中时同步 port_name
      if (transportConfig.kind === 'Serial') {
        set({
          selectedPortIndex: index,
          transportConfig: {
            kind: 'Serial',
            params: { ...transportConfig.params, port_name: ports[index].name },
          },
        });
      } else if (transportConfig.kind === 'Slcan') {
        set({
          selectedPortIndex: index,
          transportConfig: {
            kind: 'Slcan',
            params: { ...transportConfig.params, port_name: ports[index].name },
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
    if (protocolConfig.kind === 'RawData' || protocolConfig.kind === 'Slcan' || protocolConfig.kind === 'CandleLight' || protocolConfig.kind === 'LogicDecode') return;
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
          // 通道数变更 → 重新同步所有 tab 图 (ChannelSource 参数变化)
          get().controlTabs.forEach((tab) => get().syncTabGraph(tab.id));
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

  addWidget: (widget, tabId, position) => {
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
        widget.kind === 'Image' ||
        widget.kind === 'Model3D' ||
        widget.kind === 'Spectrum' ||
        widget.kind === 'Command'
      ) {
        const tabType =
          widget.kind === 'Waveform'
            ? 'waveform-extra'
            : widget.kind === 'PieChart'
            ? 'pie'
            : widget.kind === 'Image'
            ? 'image'
            : widget.kind === 'Model3D'
            ? 'model3d'
            : widget.kind === 'Spectrum'
            ? 'spectrum'
            : 'command';
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
    });
    // 同步该 tab 图到后端 (新增节点)
    get().syncTabGraph(tabId);
  },

  removeWidget: (id) => {
    const widget = get().widgets.find((w) => w.params.id === id);
    const affectedTabs = new Set<string>();
    // 找到该 widget 所在的 tab
    const node = get().rfNodes.find((n) => n.id === id);
    if (node?.data?.tabId) affectedTabs.add(node.data.tabId as string);
    set((s) => {
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
    });
    // 同步受影响的 tab 图
    affectedTabs.forEach((tabId) => get().syncTabGraph(tabId));
  },

  updateWidget: (id, widget) => {
    const node = get().rfNodes.find((n) => n.id === id);
    const tabId = node?.data?.tabId as string | undefined;
    set((s) => ({
      widgets: s.widgets.map((w) => (w.params.id === id ? widget : w)),
      rfNodes: s.rfNodes.map((n) =>
        n.id === id ? { ...n, data: { ...n.data, widget } } : n
      ),
    }));
    // widget 配置变更可能影响 NodeKind (如 Math op / input_count), 需重新同步
    if (tabId) get().syncTabGraph(tabId);
  },

  addControlTab: (name) => {
    const id = nanoid(8);
    set((s) => {
      const tabName = name || `Tab ${s.controlTabs.length + 1}`;
      const effectiveCh = getEffectiveChannels(s.protocolConfig, s.detectedChannels);
      return {
        controlTabs: [...s.controlTabs, { id, name: tabName, widgets: [] }],
        activeControlTabId: id,
        rfNodes: [...s.rfNodes, createChannelSourceNode(id, effectiveCh)],
      };
    });
    // 新 tab 仅含 ChannelSource, 同步以建立后端图
    get().syncTabGraph(id);
  },

  removeControlTab: (tabId) => {
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
    });
    // 后端清除该 tab 的图
    get().removeTabGraph(tabId);
  },

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
    // 收集受影响的 tab (从 edge 的 source/target 反查)
    const affectedTabs = new Set<string>();
    for (const ch of changes) {
      if ('source' in ch && ch.source) {
        const node = get().rfNodes.find((n) => n.id === ch.source);
        if (node?.data?.tabId) affectedTabs.add(node.data.tabId as string);
      }
      if ('target' in ch && ch.target) {
        const node = get().rfNodes.find((n) => n.id === ch.target);
        if (node?.data?.tabId) affectedTabs.add(node.data.tabId as string);
      }
    }
    set((s) => ({
      rfEdges: applyEdgeChanges(changes, s.rfEdges),
    }));
    // 同步受影响的 tab 图
    affectedTabs.forEach((tabId) => get().syncTabGraph(tabId));
  },

  onConnect: (connection) => {
    const newEdge: Edge = {
      ...connection,
      id: nanoid(8),
      animated: true,
      markerEnd: { type: MarkerType.ArrowClosed, width: 12, height: 12 },
    };
    let tabId: string | undefined;
    const sourceNode = get().rfNodes.find((n) => n.id === connection.source);
    tabId = sourceNode?.data?.tabId as string | undefined;
    // 通道源节点没有 tabId, 反查 target
    if (!tabId) {
      const targetNode = get().rfNodes.find((n) => n.id === connection.target);
      tabId = targetNode?.data?.tabId as string | undefined;
      // 通道源节点: 从 id 反查 tabId
      if (!tabId && connection.source.startsWith(CHANNEL_SOURCE_ID)) {
        tabId = connection.source.slice(CHANNEL_SOURCE_ID.length + 1);
      }
    }
    set((s) => ({
      rfEdges: addEdge(newEdge, s.rfEdges),
    }));
    if (tabId) get().syncTabGraph(tabId);
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

    // 监听 transport:can-frames 事件 (实时推送, 用于即时更新)
    // 注: 已改为仅通过 subscribe_can_frames Channel 路径推送 buffer,
    // 避免双重订阅导致重复 setState 与卡顿。
    const unlistenCanFrames = await listen<{ frames: CanFrame[] }>('transport:can-frames', (_event) => {
      // no-op: buffer 已由 Channel 路径维护
    });

    // 监听 transport:logic-samples 事件 (实时推送逻辑采样)
    const unlistenLogic = await listen<{ samples: LogicSample[] }>('transport:logic-samples', (_event) => {
      // no-op: buffer 已由 Channel 路径维护
    });

    // 监听 transport:decoded-events 事件 (实时推送解码事件)
    const unlistenDecoded = await listen<{ events: DecodedEvent[] }>('transport:decoded-events', (_event) => {
      // no-op: buffer 已由 Channel 路径维护
    });

    unlistenFns = [unlistenData, unlistenFrames, unlistenState, unlistenStats, unlistenCanFrames, unlistenLogic, unlistenDecoded];

    // 启动后端图输出订阅 (60 FPS 推送)
    // 后端在每帧评估所有 tab 的图, 并将合并快照推送至此
    if (graphOutputSub) graphOutputSub.cancel();
    graphOutputSub = subscribeGraphOutputs((snapshot) => {
      set({
        graphOutputs: snapshot.values,
        graphOutputsTick: snapshot.tick,
      });
    });

    // 启动 Custom widget 输入订阅 (30 FPS 推送)
    // 后端收集所有 Custom widget 的输入端口值, 批量推送至此
    if (customInputSub) customInputSub.cancel();
    customInputSub = subscribeCustomInputs((batch) => {
      set({ customInputs: batch.inputs });
    });

    // 启动频谱分析订阅 (30 FPS 推送)
    // 后端对每个 SpectrumSink 节点做 FFT, 推送结果至此
    if (spectrumSub) spectrumSub.cancel();
    spectrumSub = subscribeSpectrum((batch) => {
      set({ spectrumResults: batch.spectra });
    });

    // 启动 CAN 帧订阅 (后端周期性推送 can_buffer 中的最近帧)
    // buffer 内部已做 RAF 节流与引用稳定化, 组件直接订阅 canFrameBuffer
    if (canFramesSub) canFramesSub.cancel();
    canFramesSub = subscribeCanFrames((batch) => {
      canFrameBuffer.push(batch.frames);
    });

    // 启动逻辑采样订阅 (后端周期性推送 logic_buffer 中的最近采样)
    if (logicSamplesSub) logicSamplesSub.cancel();
    logicSamplesSub = subscribeLogicSamples((batch) => {
      logicSampleBuffer.push(batch.samples);
    });

    // 启动解码事件订阅 (后端周期性推送 decoded_buffer 中的最近事件)
    if (decodedEventsSub) decodedEventsSub.cancel();
    decodedEventsSub = subscribeDecodedEvents((batch) => {
      decodedEventBuffer.push(batch.events);
    });

    // 启动时同步所有现有 tab 的图到后端
    get().controlTabs.forEach((tab) => get().syncTabGraph(tab.id));

    return () => {
      unlistenFns.forEach((fn) => fn());
      unlistenFns = [];
      if (waveformSub) {
        waveformSub.cancel();
        waveformSub = null;
      }
      if (graphOutputSub) {
        graphOutputSub.cancel();
        graphOutputSub = null;
      }
      if (customInputSub) {
        customInputSub.cancel();
        customInputSub = null;
      }
      if (spectrumSub) {
        spectrumSub.cancel();
        spectrumSub = null;
      }
      if (canFramesSub) {
        canFramesSub.cancel();
        canFramesSub = null;
      }
      if (logicSamplesSub) {
        logicSamplesSub.cancel();
        logicSamplesSub = null;
      }
      if (decodedEventsSub) {
        decodedEventsSub.cancel();
        decodedEventsSub = null;
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
    case 'Filter':
      return {
        kind: 'Filter',
        params: {
          id,
          label: 'Filter',
          preset: 'Lowpass',
          cutoff: 100,
          low: 80,
          high: 200,
          sampleRate: 1000,
          precision: 3,
        },
      };
    case 'Spectrum':
      return {
        kind: 'Spectrum',
        params: {
          id,
          label: 'Spectrum',
          windowSize: 512,
          windowType: 'Hann' as WindowType,
          output: 'Magnitude' as SpectrumOutput,
          sampleRate: 1000,
        },
      };
    case 'Model3D':
      return {
        kind: 'Model3D',
        params: {
          id,
          label: 'Model3D',
          mode: 'trajectory',
          trailLength: 200,
          color: '#75beff',
          axisLength: 1.0,
        },
      };
    case 'Command':
      return {
        kind: 'Command',
        params: {
          id,
          label: 'Command',
          blocks: [
            { id: 'b1', type: 'const_hex', label: '帧头', hex: 'AA 01' },
            { id: 'b2', type: 'var_ref', label: '速度', portName: 'speed', fieldType: 'uint16LE' },
            { id: 'b3', type: 'checksum', label: '校验', checksum: 'sum8' },
          ],
          appendNewline: false,
        },
      };
  }
}

