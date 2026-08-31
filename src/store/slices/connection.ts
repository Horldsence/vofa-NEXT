import { api } from '../../lib/tauri/tauri';
import { waveformWindow } from '../../lib/buffers/dataBuffer';
import { clearRawDataTransportBuffers } from '../../lib/buffers/rawDataTransportBuffer';
import { resetPortSampleStoresForSource } from '../../lib/data/dataClient';
import { notify } from '../../lib/tauri/notifications';
import { nodeError } from '../../lib/tauri/errorGuidance';
import { t } from '../../i18n';
import { downstreamProtocolOf, type TransportNodeData, type ProtocolNodeData } from '../appStoreHelpers';
import { schemaFromProtocolConfig } from '../../lib/utils/protocolSchema';
import { useSettingsStore } from '../settingsStore';
import type { ConnectionState, PortInfo, ProtocolConfig, TransportConfig, TransportStats, WidgetBinding } from '../../types';
import type { AppSlice } from './types';

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

/// 节点统计 — rxDroppedWindow 为本窗口丢弃数 (随 transport:rx 覆盖),
/// rxDroppedTotal 为累计丢弃数 (前端累加)
export type NodeStats = TransportStats & { rxDroppedWindow: number; rxDroppedTotal: number };

export const EMPTY_NODE_STATS: NodeStats = {
  rx_bytes: 0,
  tx_bytes: 0,
  rx_frames: 0,
  tx_frames: 0,
  rx_dropped: 0,
  rxDroppedWindow: 0,
  rxDroppedTotal: 0,
};

/// 节点错误通知文案 — 统一入口 (settings 开关在 errorGuidance 内读取)

export interface ConnectionSlice {
  /// 连接状态 — 按 Transport 节点 id
  connectionStates: Record<string, ConnectionState>;
  /// 传输统计 — 按 Transport 节点 id
  nodeStats: Record<string, NodeStats>;
  /// TestData 生成开关 — 按 Transport 节点 id
  testDataRunning: Record<string, boolean>;
  ports: PortInfo[];
  refreshPorts: () => Promise<void>;
  connectNode: (nodeId: string) => Promise<void>;
  disconnectNode: (nodeId: string) => Promise<void>;
  startTestData: (nodeId: string) => Promise<void>;
  stopTestData: (nodeId: string) => Promise<void>;
  sendData: (nodeId: string, data: number[]) => Promise<void>;
  sendAndCapture: (nodeId: string, protocolNode: string, data: number[]) => Promise<void>;
  sendText: (nodeId: string, text: string) => Promise<void>;
  sendWidgetValue: (nodeId: string, protocolNode: string | null, binding: WidgetBinding, value: number) => Promise<void>;
}

export const createConnectionSlice: AppSlice<ConnectionSlice> = (set, get) => {
  return {
    connectionStates: {},
    nodeStats: {},
    testDataRunning: {},
    ports: [],
    refreshPorts: async () => {
      try {
        const ports = await api.listPorts();
        set({ ports });
      } catch (e) {
        const lang = get().lang;
        notify.error(
          t(lang, 'notifRefreshPortsFailed'),
          nodeError(lang, e),
          {
            source: 'refreshPorts',
            actions: [{ label: t(lang, 'notifRetry'), run: () => { void get().refreshPorts(); } }],
          }
        );
      }
    },

    connectNode: async (nodeId) => {
      const node = get().rfNodes.find((n) => n.id === nodeId && n.type === 'transport');
      if (!node) return;
      const config = (node.data as TransportNodeData).config;
      // TestData 生成器需要协议参数: 取字节边下游的 Protocol 节点配置, 缺省 JustFloat
      const downstreamId = downstreamProtocolOf(nodeId, get().rfEdges, get().rfNodes);
      const protocolNode = downstreamId
        ? get().rfNodes.find((n) => n.id === downstreamId)
        : undefined;
      const protocol: ProtocolConfig = protocolNode
        ? (protocolNode.data as ProtocolNodeData).config
        : { kind: 'JustFloat', channels: null };
      // schema 一并下发 (旧数据缺 schema 时按 config 回退构造; 无下游协议节点 = null)
      const schema = protocolNode
        ? ((protocolNode.data as ProtocolNodeData).schema ?? schemaFromProtocolConfig(protocol))
        : null;
      set((s) => ({
        connectionStates: { ...s.connectionStates, [nodeId]: 'Connecting' as ConnectionState },
      }));
      try {
        // 后端容量按源生效 — 连接前应用当前设置
        const cap = useSettingsStore.getState().settings.data;
        await api.setRawDataBufferCapacity(nodeId, cap.rawDataBufferBytes).catch(() => { return undefined; });
        if (downstreamId) {
          await api.setWaveformBufferCapacity(downstreamId, cap.waveformBufferPoints).catch(() => { return undefined; });
          await api.clearBuffer(downstreamId);
        }
        await api.clearRawDataBuffer(nodeId);
        clearRawDataTransportBuffers(nodeId);
        if (downstreamId) resetPortSampleStoresForSource(downstreamId);
        waveformWindow.clear();
        await api.openTransport(nodeId, config, protocol, schema);
        set((s) => ({
          connectionStates: { ...s.connectionStates, [nodeId]: 'Connected' as ConnectionState },
          testDataRunning: { ...s.testDataRunning, [nodeId]: false },
          nodeStats: { ...s.nodeStats, [nodeId]: { ...EMPTY_NODE_STATS } },
          rawDataVersion: Date.now(),
        }));
      } catch (e) {
        const lang = get().lang;
        set((s) => ({
          connectionStates: { ...s.connectionStates, [nodeId]: 'Error' as ConnectionState },
        }));
        notify.error(
          t(lang, 'notifConnectFailed'),
          nodeError(lang, e),
          {
            source: 'connect',
            actions: [{ label: t(lang, 'notifRetry'), run: () => { void get().connectNode(nodeId); } }],
          }
        );
      }
    },

    disconnectNode: async (nodeId) => {
      const downstreamId = downstreamProtocolOf(nodeId, get().rfEdges, get().rfNodes);
      set((s) => ({
        connectionStates: { ...s.connectionStates, [nodeId]: 'Disconnected' as ConnectionState },
        testDataRunning: { ...s.testDataRunning, [nodeId]: false },
      }));
      clearRawDataTransportBuffers(nodeId);
      if (downstreamId) resetPortSampleStoresForSource(downstreamId);
      try {
        await api.closeTransport(nodeId);
      } catch (e) {
        const lang = get().lang;
        set((s) => ({
          connectionStates: { ...s.connectionStates, [nodeId]: 'Error' as ConnectionState },
        }));
        notify.error(
          t(lang, 'notifDisconnectFailed'),
          nodeError(lang, e),
          {
            source: 'disconnect',
            actions: [{ label: t(lang, 'notifRetry'), run: () => { void get().disconnectNode(nodeId); } }],
          }
        );
      }
    },

    startTestData: async (nodeId) => {
      try {
        await api.startTestData(nodeId);
        set((s) => ({ testDataRunning: { ...s.testDataRunning, [nodeId]: true } }));
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifStartTestDataFailed'), nodeError(lang, e), { source: 'startTestData' });
      }
    },

    stopTestData: async (nodeId) => {
      try {
        await api.stopTestData(nodeId);
        set((s) => ({ testDataRunning: { ...s.testDataRunning, [nodeId]: false } }));
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifStopTestDataFailed'), nodeError(lang, e), { source: 'stopTestData' });
      }
    },

    sendData: async (nodeId, data) => {
      try {
        await api.sendRaw(nodeId, data);
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifSendFailed'), nodeError(lang, e), { source: 'sendData' });
      }
    },

    sendAndCapture: async (nodeId, protocolNode, data) => {
      try {
        const result = await api.sendAndCapture(nodeId, protocolNode, data);
        set((s) => ({
          widgets: s.widgets.map((w) => {
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
        notify.error(t(lang, 'notifSendFailed'), nodeError(lang, e), { source: 'sendAndCapture' });
      }
    },

    sendText: async (nodeId, text) => {
      try {
        await api.sendString(nodeId, text);
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifSendFailed'), nodeError(lang, e), { source: 'sendText' });
      }
    },

    sendWidgetValue: async (nodeId, protocolNode, binding, value) => {
      try {
        await api.sendWidgetValue(nodeId, protocolNode, binding, value);
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifSendFailed'), nodeError(lang, e), { source: 'sendWidget' });
      }
    },
  };
}
