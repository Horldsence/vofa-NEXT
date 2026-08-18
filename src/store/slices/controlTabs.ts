import { nanoid } from 'nanoid';

export interface ControlTabSlice {
  controlTabs: { id: string; name: string; widgets: string[] }[];
  activeControlTabId: string;
  addControlTab: (name?: string) => void;
  removeControlTab: (tabId: string) => void;
  setActiveControlTab: (tabId: string) => void;
  renameControlTab: (tabId: string, name: string) => void;
}

export function createControlTabSlice(set: any, get: any): ControlTabSlice {
  return {
    controlTabs: [{ id: 'default', name: 'Tab 1', widgets: [] }],
    activeControlTabId: 'default',

    addControlTab: (name) => {
      const id = nanoid(8);
      set((s: any) => {
        const tabName = name || `Tab ${s.controlTabs.length + 1}`;
        return {
          controlTabs: [...s.controlTabs, { id, name: tabName, widgets: [] }],
          activeControlTabId: id,
        };
      });
      get().syncTabGraph(id);
    },

    removeControlTab: (tabId) => {
      set((s: any) => {
        const remaining = s.controlTabs.filter((t: any) => t.id !== tabId);
        if (remaining.length === 0) {
          const defaultTab = { id: 'default', name: 'Tab 1', widgets: [] };
          return {
            controlTabs: [defaultTab],
            activeControlTabId: 'default',
          };
        }
        const tabNodeIds = new Set(
          s.rfNodes.filter((n: any) => n.data.tabId === tabId).map((n: any) => n.id)
        );
        return {
          controlTabs: remaining,
          activeControlTabId:
            s.activeControlTabId === tabId ? remaining[0].id : s.activeControlTabId,
          // 全局节点 (data.global) 不属于任何 tab, 不随 tab 删除
          rfNodes: s.rfNodes.filter((n: any) => n.data.tabId !== tabId),
          rfEdges: s.rfEdges.filter((e: any) => !tabNodeIds.has(e.source) && !tabNodeIds.has(e.target)),
        };
      });
      get().removeTabGraph(tabId);
    },

    setActiveControlTab: (tabId) => set({ activeControlTabId: tabId }),

    renameControlTab: (tabId, name) =>
      set((s: any) => ({
        controlTabs: s.controlTabs.map((t: any) =>
          t.id === tabId ? { ...t, name } : t
        ),
      })),
  };
}
