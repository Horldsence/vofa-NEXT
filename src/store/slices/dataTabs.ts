import { t } from '../../i18n';
import type { DataTab } from '../../types';

export interface DataTabSlice {
  dataTabs: DataTab[];
  activeDataTabId: string;
  addDataTab: (tab: DataTab) => void;
  removeDataTab: (tabId: string) => void;
  setActiveDataTab: (tabId: string) => void;
  addCanTab: () => void;
  addLogicTab: () => void;
}

export function createDataTabSlice(set: any, get: any): DataTabSlice {
  return {
    dataTabs: [
      { id: 'waveform-fixed', type: 'waveform' as const, name: 'Waveform', closable: false },
      { id: 'raw-fixed', type: 'raw' as const, name: 'Raw Data', closable: false },
    ],
    activeDataTabId: 'waveform-fixed',

    addDataTab: (tab) =>
      set((s: any) => ({
        dataTabs: [...s.dataTabs, tab],
        activeDataTabId: tab.id,
      })),

    removeDataTab: (tabId) =>
      set((s: any) => {
        const tab = s.dataTabs.find((t: DataTab) => t.id === tabId);
        if (!tab || !tab.closable) return s;
        const remaining = s.dataTabs.filter((t: DataTab) => t.id !== tabId);
        return {
          dataTabs: remaining,
          activeDataTabId:
            s.activeDataTabId === tabId ? remaining[0]?.id ?? 'waveform-fixed' : s.activeDataTabId,
        };
      }),

    setActiveDataTab: (tabId) => set({ activeDataTabId: tabId }),

    addCanTab: () => {
      const existing = get().dataTabs.find((t: DataTab) => t.type === 'can');
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

    addLogicTab: () => {
      const existing = get().dataTabs.find((t: DataTab) => t.type === 'logic');
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
  };
}
