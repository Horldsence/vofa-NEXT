import { api } from '../../lib/tauri/tauri';
import { notify } from '../../lib/tauri/notifications';
import { nodeError } from '../../lib/tauri/errorGuidance';
import { t } from '../../i18n';
import type { ProtocolConfig, ProtocolSchema } from '../../types';
import { getEffectiveChannels, type ProtocolNodeData } from '../appStoreHelpers';
import { schemaFromProtocolConfig, schemaPortNames } from '../../lib/utils/protocolSchema';
import {
  withHistoryOp,
  beginHistoryOp,
  commitHistoryOp,
  type HistoryTarget,
} from '../historyStore';

/** 协议类操作目标 — 行首徽章用画布 Protocol 节点同款 (主题色 Binary) */
const protocolTarget = (): HistoryTarget => ({ kind: 'node', node: { kind: 'protocol' } });

export const DEFAULT_PROTOCOL: ProtocolConfig = {
  kind: 'JustFloat',
  channels: null,
};

export interface ProtocolSlice {
  /// 自动检测到的通道数 — 按 Protocol 节点 id (仅自动模式有值, 由后端
  /// `protocol:channels-detected` 事件驱动写入)
  detectedChannels: Record<string, number | null>;
  /// 更新 Protocol 节点配置 (节点 data + 后端运行时引擎 + 全 tab 图同步)
  /// 预设路径下 schema 随 config 工厂重建; custom schema 保持 (由 setProtocolNodeSchema 管理)
  setProtocolNodeConfig: (nodeId: string, config: ProtocolConfig) => Promise<void>;
  /// 更新 Protocol 节点的协议转换目标 (null = 无转换)
  setProtocolNodeConvertTo: (nodeId: string, convertTo: ProtocolConfig | null) => void;
  /// 更新 Protocol 节点的帧 schema (custom 块编辑 / 重置为预设)
  setProtocolNodeSchema: (nodeId: string, schema: ProtocolSchema) => void;
}

export function createProtocolSlice(set: any, get: any): ProtocolSlice {
  return {
    detectedChannels: {},

    setProtocolNodeConfig: async (nodeId, config) => {
      // 1. 更新节点 data (通道数立即按新配置重算)
      beginHistoryOp();
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
      // 文档变更已在上面同步完成 — 立即提交历史 (await 之后的后端调用不属于文档态)
      commitHistoryOp(
        { opKey: 'opUpdateProtocolConfig', detailText: config.kind, target: protocolTarget() },
        { coalesceKey: `protocol.config.${nodeId}` }
      );
      // 2. 后端运行时引擎 (图同步是权威, 此调用让引擎立即重建;
      //    手动模式 buffer 通道数 + 推送记录复位都在 set_protocol 内一并完成,
      //    自动模式由后端 protocol:channels-detected 事件驱动前端响应)
      try {
        await api.setProtocol(nodeId, config);
        if ((config.kind === 'JustFloat' || config.kind === 'FireWater') && config.channels != null) {
          set((s: any) => ({ detectedChannels: { ...s.detectedChannels, [nodeId]: null } }));
        }
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifSetProtocolFailed'), nodeError(lang, e), { source: 'setProtocol' });
      }
      // 3. 全 tab 图同步
      get().controlTabs.forEach((tab: any) => get().syncTabGraph(tab.id));
    },

    setProtocolNodeConvertTo: (nodeId, convertTo) =>
      withHistoryOp(
        {
          opKey: 'opConvertProtocolTo',
          detailText: convertTo?.kind ?? undefined,
          target: protocolTarget(),
        },
        () => {
          set((s: any) => ({
            rfNodes: s.rfNodes.map((n: any) =>
              n.id === nodeId && n.type === 'protocol'
                ? { ...n, data: { ...n.data, convertTo } }
                : n
            ),
          }));
          get().controlTabs.forEach((tab: any) => get().syncTabGraph(tab.id));
        }
      ),

    setProtocolNodeSchema: (nodeId, schema) =>
      withHistoryOp({ opKey: 'opUpdateProtocolSchema', target: protocolTarget() }, () => {
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
      }),
  };
}
