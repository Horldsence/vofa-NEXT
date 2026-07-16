import { create } from 'zustand';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '../lib/tauri';
import { waveformBuffer, rawDataBuffer } from '../lib/dataBuffer';
import { nanoid } from 'nanoid';
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

const DEFAULT_PROTOCOL: ProtocolConfig = {
  kind: 'JustFloat',
  channels: 4,
};

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

  // Widgets
  widgets: WidgetConfig[];

  // Raw data version (for triggering re-renders)
  rawDataVersion: number;

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

  // Widget management
  addWidget: (widget: WidgetConfig) => void;
  removeWidget: (id: string) => void;
  updateWidget: (id: string, widget: WidgetConfig) => void;

  // Data
  clearData: () => void;

  // Event setup
  initEventListeners: () => Promise<() => void>;
}

let unlistenFns: UnlistenFn[] = [];
let flushTimer: ReturnType<typeof setTimeout> | null = null;

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

  widgets: [],

  rawDataVersion: 0,

  refreshPorts: async () => {
    try {
      const ports = await api.listPorts();
      set({ ports });
    } catch (e) {
      console.error('Failed to list ports:', e);
    }
  },

  connect: async () => {
    const { transportConfig, protocolConfig } = get();
    try {
      await api.setProtocol(protocolConfig);
      waveformBuffer.setChannels(
        'channels' in protocolConfig ? protocolConfig.channels : 4
      );
      // 清空旧数据并重置统计
      waveformBuffer.clear();
      rawDataBuffer.clear();
      await api.openTransport(transportConfig);
      set({
        connectionState: 'Connected',
        stats: { rx_bytes: 0, tx_bytes: 0, rx_frames: 0, tx_frames: 0 },
        rawDataVersion: Date.now(),
      });
    } catch (e) {
      console.error('Connect failed:', e);
      set({ connectionState: 'Error' });
    }
  },

  disconnect: async () => {
    try {
      await api.closeTransport();
      set({ connectionState: 'Disconnected' });
    } catch (e) {
      console.error('Disconnect failed:', e);
    }
  },

  setTransportConfig: (config) => set({ transportConfig: config }),

  setProtocolConfig: async (config) => {
    set({ protocolConfig: config });
    try {
      await api.setProtocol(config);
      waveformBuffer.setChannels(
        'channels' in config ? config.channels : 4
      );
    } catch (e) {
      console.error('Set protocol failed:', e);
    }
  },

  sendData: async (data) => {
    try {
      await api.sendRaw(data);
    } catch (e) {
      console.error('Send failed:', e);
    }
  },

  sendText: async (text) => {
    try {
      await api.sendString(text);
    } catch (e) {
      console.error('Send failed:', e);
    }
  },

  sendWidgetValue: async (binding, value) => {
    try {
      await api.sendWidgetValue(binding, value);
    } catch (e) {
      console.error('Send widget value failed:', e);
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

  addWidget: (widget) => set((s) => ({ widgets: [...s.widgets, widget] })),

  removeWidget: (id) =>
    set((s) => ({ widgets: s.widgets.filter((w) => w.params.id !== id) })),

  updateWidget: (id, widget) =>
    set((s) => ({
      widgets: s.widgets.map((w) => (w.params.id === id ? widget : w)),
    })),

  clearData: () => {
    waveformBuffer.clear();
    rawDataBuffer.clear();
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

    const unlistenFrame = await listen<DataFrame>('transport:frame', (event) => {
      waveformBuffer.push(event.payload);
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

    unlistenFns = [unlistenData, unlistenFrame, unlistenState, unlistenStats];

    return () => {
      unlistenFns.forEach((fn) => fn());
      unlistenFns = [];
    };
  },
}));

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
  }
}
