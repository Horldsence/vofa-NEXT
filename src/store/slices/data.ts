import { api } from '../../lib/tauri/tauri';
import { waveformWindow, rawDataBuffer } from '../../lib/buffers/dataBuffer';
import { isGlobalNode } from '../appStoreHelpers';
import { notify, formatError } from '../../lib/tauri/notifications';
import { t } from '../../i18n';

export interface DataSlice {
  rawDataVersion: number;
  clearData: () => Promise<void>;
}

export function createDataSlice(set: any, get: any): DataSlice {
  return {
    rawDataVersion: 0,

    clearData: async () => {
      try {
        // 波形缓冲区按 Protocol 节点分实例 — 逐源清空
        const protocolIds: string[] = get().rfNodes
          .filter((n: any) => n.type === 'protocol' && isGlobalNode(n))
          .map((n: any) => n.id);
        await Promise.all(protocolIds.map((id) => api.clearBuffer(id)));
        // 缺省清空全部 Transport 源的原始数据收集器
        await api.clearRawDataBuffer();
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifClearBufferFailed'), formatError(e), { source: 'clearBuffer' });
      }
      rawDataBuffer.clear();
      waveformWindow.clear();
      set({ rawDataVersion: Date.now() });
    },
  };
}
