import { api } from '../../lib/tauri';
import { waveformWindow, rawDataBuffer } from '../../lib/dataBuffer';
import { notify, formatError } from '../../lib/notifications';
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
        await api.clearBuffer();
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
