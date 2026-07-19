import { api } from '../../lib/tauri';
import { notify, formatError } from '../../lib/notifications';
import { t } from '../../i18n';
import type { ProtocolConfig } from '../../types';
import { detectedChannelsPoller, setDetectedChannelsPoller } from './connection';
import { getEffectiveChannels } from '../appStoreHelpers';

export const DEFAULT_PROTOCOL: ProtocolConfig = {
  kind: 'JustFloat',
  channels: null,
};

export interface ProtocolSlice {
  protocolConfig: ProtocolConfig;
  detectedChannels: number | null;
  setProtocolConfig: (config: ProtocolConfig) => Promise<void>;
  pollDetectedChannels: () => Promise<void>;
}

export function createProtocolSlice(set: any, get: any): ProtocolSlice {
  return {
    protocolConfig: DEFAULT_PROTOCOL,
    detectedChannels: null,

    setProtocolConfig: async (config) => {
      set({ protocolConfig: config });
      try {
        await api.setProtocol(config);
        if ((config.kind === 'JustFloat' || config.kind === 'FireWater') && config.channels != null) {
          await api.setBufferChannels(config.channels);
          set({ detectedChannels: null });
        }
      } catch (e) {
        const lang = get().lang;
        notify.error(t(lang, 'notifSetProtocolFailed'), formatError(e), { source: 'setProtocol' });
      }
    },

    pollDetectedChannels: async () => {
      const { protocolConfig } = get();
      if (protocolConfig.kind === 'RawData' || protocolConfig.kind === 'Slcan' || protocolConfig.kind === 'CandleLight' || protocolConfig.kind === 'LogicDecode') return;
      if (protocolConfig.channels != null) return;

      if (detectedChannelsPoller) clearInterval(detectedChannelsPoller);

      const newPoller = setInterval(async () => {
        try {
          const detected = await api.getDetectedChannels();
          const prev = get().detectedChannels;
          if (detected !== prev) {
            set({ detectedChannels: detected });
            const effective = getEffectiveChannels(protocolConfig, detected);
            set((s: any) => ({
              rfNodes: s.rfNodes.map((n: any) =>
                n.type === 'channelSource' && n.data.tabId
                  ? { ...n, data: { ...n.data, channelCount: effective } }
                  : n
              ),
            }));
            get().controlTabs.forEach((tab: any) => get().syncTabGraph(tab.id));
          }
        } catch (e) {
          const lang = get().lang;
          notify.warn(t(lang, 'notifPollChannelsFailed'), formatError(e), { source: 'pollChannels' });
        }
      }, 1000);

      setDetectedChannelsPoller(newPoller);
    },
  };
}
