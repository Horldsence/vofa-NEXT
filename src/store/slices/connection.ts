import { api } from '../../lib/tauri/tauri';
import { rawDataBuffer, waveformWindow } from '../../lib/buffers/dataBuffer';
import { notify } from '../../lib/tauri/notifications';
import { nodeErrorText } from '../../lib/tauri/errorGuidance';
import { t, type Lang } from '../../i18n';
import { downstreamProtocolOf, type TransportNodeData, type ProtocolNodeData } from '../appStoreHelpers';
import { schemaFromProtocolConfig } from '../../lib/utils/protocolSchema';
import { useSettingsStore } from '../settingsStore';
import type { ConnectionState, PortInfo, ProtocolConfig, TransportConfig, TransportStats, WidgetBinding } from '../../types';

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

/// 节点错误通知文案 — 按错误枚举解析, 每种类型首次出错时追加排查引导
/// (遵循 settings.general.showContextualTips 开关)
function nodeError(lang: Lang, e: unknown): string {
  const tips = useSettingsStore.getState().settings.general.showContextualTips;
  return nodeErrorText(lang, e, tips);
}

export interface ConnectionSlice {
  /// 连接状态 — 按 Transport 节点 id
  connectionStates: Record<string, ConnectionState>;
  /// 传输统计 — 按 Transport 节点 id
  nodeStats: Record<string, NodeStats>;
  /// TestData 生成开关 — 按 Transport 节点 id
  testDataRunning: Record<string, boolean>;
  ports: PortInfo[];
  /// RawData 视图选中的字节源 (Transport 节点 id; null = 自动选第一个)
  rawDataSourceNodeId: string | null;

  refreshPorts: () => Promise<void>;
  connectNode: (nodeId: string) => Promise<void>;
  disconnectNode: (nodeId: string) => Promise<void>;
  startTestData: (nodeId: string) => Promise<void>;
  stopTestData: (nodeId: string) => Promise<void>;
  sendData: (nodeId: string, data: number[]) => Promise<void>;
  sendAndCapture: (nodeId: string, protocolNode: string, data: number[]) => Promise<void>;
  sendText: (nodeId: string, text: string) => Promise<void>;
  sendWidgetValue: (nodeId: string, protocolNode: string | null, binding: WidgetBinding, value: number) => Promise<void>;
  setRawDataSourceNodeId: (nodeId: string | null) => void;
}

export function createConnectionSlice(set: any, get: any): ConnectionSlice {
  return {
    connectionStates: {},
    nodeStats: {},
    testDataRunning: {},
    ports: [],
    rawDataSourceNodeId: null,

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
      const node = get().rfNodes.find((n: any) => n.id === nodeId && n.type === 'transport');
      if (!node) return;
      const config = (node.data as TransportNodeData).config;
      // TestData 生成器需要协议参数: 取字节边下游的 Protocol 节点配置, 缺省 JustFloat
      const downstreamId = downstreamProtocolOf(nodeId, get().rfEdges, get().rfNodes);
      const protocolNode = downstreamId
        ? get().rfNodes.find((n: any) => n.id === downstreamId)
        : undefined;
      const protocol: ProtocolConfig = protocolNode
        ? (protocolNode.data as ProtocolNodeData).config
        : { kind: 'JustFloat', channels: null };
      // schema 一并下发 (旧数据缺 schema 时按 config 回退构造; 无下游协议节点 = null)
      const schema = protocolNode
        ? ((protocolNode.data as ProtocolNodeData).schema ?? schemaFromProtocolConfig(protocol))
        : null;
      try {
        // 后端容量按源生效 — 连接前应用当前设置
        const cap = useSettingsStore.getState().settings.data;
        await api.setRawDataBufferCapacity(nodeId, cap.rawDataBufferBytes).catch(() => {});
        if (downstreamId) {
          await api.setWaveformBufferCapacity(downstreamId, cap.waveformBufferPoints).catch(() => {});
          await api.clearBuffer(downstreamId);
        }
        await api.clearRawDataBuffer(nodeId);
        rawDataBuffer.clear();
        waveformWindow.clear();
        await api.openTransport(nodeId, config, protocol, schema);
        set((s: any) => ({
          connectionStates: { ...s.connectionStates, [nodeId]: 'Connected' as ConnectionState },
          testDataRunning: { ...s.testDataRunning, [nodeId]: false },
          nodeStats: { ...s.nodeStats, [nodeId]: { ...EMPTY_NODE_STATS } },
          rawDataVersion: Date.now(),
        }));
      } catch (e) {
        const lang = get().lang;
        set((s: any) => ({
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
      try {
        await api.closeTransport(nodeId);
        set((s: any) => ({
          connectionStates: { ...s.connectionStates, [nodeId]: 'Disconnected' as ConnectionState },
          testDataRunning: { ...s.testDataRunning, [nodeId]: false },
        }));
      } catch (e) {
        const lang = get().lang;
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
        set((s: any) => ({ testDataRunning: { ...s.testDataRunning, [nodeId]: true } }));
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifStartTestDataFailed'), nodeError(lang, e), { source: 'startTestData' });
      }
    },

    stopTestData: async (nodeId) => {
      try {
        await api.stopTestData(nodeId);
        set((s: any) => ({ testDataRunning: { ...s.testDataRunning, [nodeId]: false } }));
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

    setRawDataSourceNodeId: (nodeId) => set({ rawDataSourceNodeId: nodeId }),
  };
}
