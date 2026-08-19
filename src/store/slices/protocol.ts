import { api } from '../../lib/tauri/tauri';
import { notify } from '../../lib/tauri/notifications';
import { nodeErrorText } from '../../lib/tauri/errorGuidance';
import { t, type Lang } from '../../i18n';
import { useSettingsStore } from '../settingsStore';
import type { ProtocolConfig, ProtocolSchema } from '../../types';
import { cleanupDetectedChannelsPollers, detectedChannelsPollers, setDetectedChannelsPoller } from './connection';
import { getEffectiveChannels, type ProtocolNodeData } from '../appStoreHelpers';
import { schemaFromProtocolConfig, schemaPortNames } from '../../lib/utils/protocolSchema';

/// 节点错误通知文案 — 按错误枚举解析, 每种类型首次出错时追加排查引导
/// (遵循 settings.general.showContextualTips 开关)
function nodeError(lang: Lang, e: unknown): string {
  const tips = useSettingsStore.getState().settings.general.showContextualTips;
  return nodeErrorText(lang, e, tips);
}

export const DEFAULT_PROTOCOL: ProtocolConfig = {
  kind: 'JustFloat',
  channels: null,
};

export interface ProtocolSlice {
  /// 自动检测到的通道数 — 按 Protocol 节点 id (仅自动模式有值)
  detectedChannels: Record<string, number | null>;
  /// 更新 Protocol 节点配置 (节点 data + 后端运行时引擎 + 全 tab 图同步)
  /// 预设路径下 schema 随 config 工厂重建; custom schema 保持 (由 setProtocolNodeSchema 管理)
  setProtocolNodeConfig: (nodeId: string, config: ProtocolConfig) => Promise<void>;
  /// 更新 Protocol 节点的协议转换目标 (null = 无转换)
  setProtocolNodeConvertTo: (nodeId: string, convertTo: ProtocolConfig | null) => void;
  /// 更新 Protocol 节点的帧 schema (custom 块编辑 / 重置为预设)
  setProtocolNodeSchema: (nodeId: string, schema: ProtocolSchema) => void;
  /// 按图中 Protocol 节点重建自动检测轮询 (仅自动模式的 JustFloat/FireWater 需要)
  ensureChannelsPolling: () => void;
}

function needsPolling(config: ProtocolConfig): boolean {
  return (config.kind === 'JustFloat' || config.kind === 'FireWater') && config.channels == null;
}

export function createProtocolSlice(set: any, get: any): ProtocolSlice {
  return {
    detectedChannels: {},

    setProtocolNodeConfig: async (nodeId, config) => {
      // 1. 更新节点 data (通道数立即按新配置重算)
      const detected = get().detectedChannels[nodeId] ?? null;
      const effective = getEffectiveChannels(config, detected);
      // schema 联动: 预设 (或缺失) → 按新 config 工厂重建; custom → 保留用户块
      const prevNode = get().rfNodes.find((n: any) => n.id === nodeId);
      const prevSchema = prevNode ? (prevNode.data as ProtocolNodeData).schema : undefined;
      const schema = prevSchema?.preset === 'custom' ? prevSchema : schemaFromProtocolConfig(config);
      set((s: any) => ({
        rfNodes: s.rfNodes.map((n: any) =>
          n.id === nodeId && n.type === 'protocol'
            ? { ...n, data: { ...n.data, config, channels: effective, schema, label: config.kind } }
            : n
        ),
      }));
      // 2. 后端运行时引擎 (图同步是权威, 此调用让引擎立即重建)
      try {
        await api.setProtocol(nodeId, config);
        if ((config.kind === 'JustFloat' || config.kind === 'FireWater') && config.channels != null) {
          await api.setBufferChannels(nodeId, config.channels);
          set((s: any) => ({ detectedChannels: { ...s.detectedChannels, [nodeId]: null } }));
        }
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifSetProtocolFailed'), nodeError(lang, e), { source: 'setProtocol' });
      }
      // 3. 全 tab 图同步 + 轮询重建
      get().controlTabs.forEach((tab: any) => get().syncTabGraph(tab.id));
      get().ensureChannelsPolling();
    },

    setProtocolNodeConvertTo: (nodeId, convertTo) => {
      set((s: any) => ({
        rfNodes: s.rfNodes.map((n: any) =>
          n.id === nodeId && n.type === 'protocol'
            ? { ...n, data: { ...n.data, convertTo } }
            : n
        ),
      }));
      get().controlTabs.forEach((tab: any) => get().syncTabGraph(tab.id));
    },

    setProtocolNodeSchema: (nodeId, schema) => {
      set((s: any) => ({
        rfNodes: s.rfNodes.map((n: any) => {
          if (n.id !== nodeId || n.type !== 'protocol') return n;
          // custom 下节点端口数跟随 decode 块派生 (摘要显示用; 端口名以 protocolPortNames 为准)
          const channels = schema.preset === 'custom'
            ? schemaPortNames(schema.decode).length
            : (n.data as ProtocolNodeData).channels;
          return { ...n, data: { ...n.data, schema, channels } };
        }),
      }));
      // 图同步为权威 (NodeKind::Protocol.schema 随图下发, 引擎按 schema 重建)
      get().controlTabs.forEach((tab: any) => get().syncTabGraph(tab.id));
    },

    ensureChannelsPolling: () => {
      const protocolNodes = get().rfNodes.filter((n: any) => n.type === 'protocol' && n.data?.global);
      const want = new Set<string>(
        protocolNodes
          .filter((n: any) => needsPolling((n.data as ProtocolNodeData).config))
          .map((n: any) => n.id as string)
      );
      // 停止多余轮询
      for (const id of Object.keys(detectedChannelsPollers)) {
        if (!want.has(id)) setDetectedChannelsPoller(id, null);
      }
      // 启动缺失轮询
      for (const id of want) {
        if (detectedChannelsPollers[id]) continue;
        const poller = setInterval(async () => {
          try {
            const detected = await api.getDetectedChannels(id);
            const prev = get().detectedChannels[id] ?? null;
            if (detected === prev) return;
            set((s: any) => ({ detectedChannels: { ...s.detectedChannels, [id]: detected } }));
            const node = get().rfNodes.find((n: any) => n.id === id);
            if (!node) return;
            const config = (node.data as ProtocolNodeData).config;
            const effective = getEffectiveChannels(config, detected);
            await api.setBufferChannels(id, effective);
            // 通道数变化 → 更新节点 ch 端口数并 re-sync 所有 tab (ProtocolSource 通道数随之变化)
            set((s: any) => ({
              rfNodes: s.rfNodes.map((n: any) =>
                n.id === id ? { ...n, data: { ...n.data, channels: effective } } : n
              ),
            }));
            get().controlTabs.forEach((tab: any) => get().syncTabGraph(tab.id));
          } catch (e) {
            const lang = get().lang;
            notify.warn(t(lang, 'notifPollChannelsFailed'), nodeError(lang, e), { source: 'pollChannels' });
          }
        }, 1000);
        setDetectedChannelsPoller(id, poller);
      }
      // 无节点时需要手动模式缓冲通道的场景已由 setProtocolNodeConfig 覆盖
      if (protocolNodes.length === 0) cleanupDetectedChannelsPollers();
    },
  };
}
