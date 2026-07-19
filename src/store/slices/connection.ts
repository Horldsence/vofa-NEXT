import { type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '../../lib/tauri';
import { waveformWindow, rawDataBuffer } from '../../lib/dataBuffer';
import { notify, formatError } from '../../lib/notifications';
import { t } from '../../i18n';
import type { ConnectionState, PortInfo, TransportConfig, TransportStats, WidgetBinding } from '../../types';
import type { SidebarView } from './sidebar';

export const DEFAULT_SERIAL: TransportConfig = {
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

/// 波形数据订阅 (connect 时启动, disconnect 时取消)
export let waveformSub: { cancel: () => void } | null = null;
/// 通道自动检测轮询器 (自动模式下启动)
export let detectedChannelsPoller: ReturnType<typeof setInterval> | null = null;
/// initEventListeners 注册的 Tauri 事件监听 (用于 cleanup)
export let unlistenFns: UnlistenFn[] = [];

export function setWaveformSub(sub: { cancel: () => void } | null) {
  waveformSub = sub;
}
export function cleanupWaveformSub() {
  if (waveformSub) { waveformSub.cancel(); waveformSub = null; }
}
export function setDetectedChannelsPoller(poller: ReturnType<typeof setInterval> | null) {
  detectedChannelsPoller = poller;
}
export function cleanupDetectedChannelsPoller() {
  if (detectedChannelsPoller) { clearInterval(detectedChannelsPoller); detectedChannelsPoller = null; }
}
export function setUnlistenFns(fns: UnlistenFn[]) {
  unlistenFns = fns;
}

export interface ConnectionSlice {
  connectionState: ConnectionState;
  stats: TransportStats;
  transportConfig: TransportConfig;
  ports: PortInfo[];
  selectedPortIndex: number;
  testDataRunning: boolean;

  refreshPorts: () => Promise<void>;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  setTransportConfig: (config: TransportConfig) => void;
  startTestData: () => Promise<void>;
  stopTestData: () => Promise<void>;
  sendData: (data: number[]) => Promise<void>;
  sendAndCapture: (data: number[]) => Promise<void>;
  sendText: (text: string) => Promise<void>;
  sendWidgetValue: (binding: WidgetBinding, value: number) => Promise<void>;
  selectPort: (index: number) => void;
}

export function createConnectionSlice(set: any, get: any): ConnectionSlice {
  return {
    connectionState: 'Disconnected',
    stats: { rx_bytes: 0, tx_bytes: 0, rx_frames: 0, tx_frames: 0 },
    transportConfig: DEFAULT_SERIAL,
    ports: [],
    selectedPortIndex: -1,
    testDataRunning: false,

    refreshPorts: async () => {
      try {
        const ports = await api.listPorts();
        const { transportConfig, selectedPortIndex } = get();
        const isSerialLike = transportConfig.kind === 'Serial' || transportConfig.kind === 'Slcan';
        const currentName = isSerialLike
          ? (transportConfig.params as { port_name: string }).port_name
          : '';
        let newIndex = -1;
        if (currentName) {
          newIndex = ports.findIndex((p) => p.name === currentName);
        } else if (selectedPortIndex >= 0 && selectedPortIndex < ports.length) {
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
          sidebarView: 'protocol' as SidebarView,
          sidebarVisible: true,
          testDataRunning: false,
          stats: { rx_bytes: 0, tx_bytes: 0, rx_frames: 0, tx_frames: 0 },
          rawDataVersion: Date.now(),
        });
        if (waveformSub) {
          waveformSub.cancel();
        }
        waveformSub = api.subscribeWaveform(
          (window) => waveformWindow.set(window),
          { intervalMs: 33, maxPoints: 2000 }
        );
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

    sendData: async (data) => {
      try {
        await api.sendRaw(data);
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifSendFailed'), formatError(e), { source: 'sendData' });
      }
    },

    sendAndCapture: async (data) => {
      try {
        const result = await api.sendAndCapture(data);
        set((s: any) => ({
          widgets: s.widgets.map((w: any) => {
            if (w.kind !== 'Command' || !w.params.loopbackEnabled) return w;
            return {
              ...w,
              params: {
                ...w.params,
                loopbackHistory: [
                  ...(w.params.loopbackHistory ?? []),
                  result,
                ].slice(-200),
              },
            };
          }),
        }));
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifSendFailed'), formatError(e), { source: 'sendAndCapture' });
      }
    },

    sendText: async (text) => {
      try {
        await api.sendString(text);
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifSendFailed'), formatError(e), { source: 'sendText' });
      }
    },

    sendWidgetValue: async (binding, value) => {
      try {
        await api.sendWidgetValue(binding, value);
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifSendFailed'), formatError(e), { source: 'sendWidget' });
      }
    },

    selectPort: (index) => {
      const { ports, transportConfig } = get();
      if (index >= 0 && index < ports.length) {
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
  };
}
