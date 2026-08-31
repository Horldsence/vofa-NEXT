import { nanoid } from 'nanoid';
import { withHistoryOp, type HistoryTarget } from '../historyStore';
import type { AppSlice } from './types';

/// 页签级操作的目标语义 (中性徽章)
const tabTarget = (): HistoryTarget => ({ kind: 'tab' });

export interface ControlTabSlice {
  controlTabs: { id: string; name: string; widgets: string[] }[];
  activeControlTabId: string;
  addControlTab: (name?: string) => void;
  removeControlTab: (tabId: string) => void;
  setActiveControlTab: (tabId: string) => void;
  renameControlTab: (tabId: string, name: string) => void;
}

export const createControlTabSlice: AppSlice<ControlTabSlice> = (set, get) => {
  return {
    controlTabs: [{ id: 'default', name: 'Tab 1', widgets: [] }],
    activeControlTabId: 'default',

    addControlTab: (name) =>
      withHistoryOp({ opKey: 'opAddControlTab', target: tabTarget() }, () => {
        const id = nanoid(8);
        set((s) => {
          const tabName = name ?? `Tab ${s.controlTabs.length + 1}`;
          return {
            controlTabs: [...s.controlTabs, { id, name: tabName, widgets: [] }],
            activeControlTabId: id,
          };
        });
        void get().syncTabGraph(id);
      }),

    removeControlTab: (tabId) =>
      withHistoryOp(
        {
          opKey: 'opRemoveControlTab',
          detailText: get().controlTabs.find((t) => t.id === tabId)?.name,
          target: tabTarget(),
        },
        () => {
          set((s) => {
            const remaining = s.controlTabs.filter((t) => t.id !== tabId);
            if (remaining.length === 0) {
              const defaultTab = { id: 'default', name: 'Tab 1', widgets: [] };
              return {
                controlTabs: [defaultTab],
                activeControlTabId: 'default',
              };
            }
            const tabNodeIds = new Set(
              s.rfNodes.filter((n) => n.data.tabId === tabId).map((n) => n.id)
            );
            return {
              controlTabs: remaining,
              activeControlTabId:
                s.activeControlTabId === tabId ? remaining[0].id : s.activeControlTabId,
              // 全局节点 (data.global) 不属于任何 tab, 不随 tab 删除
              rfNodes: s.rfNodes.filter((n) => n.data.tabId !== tabId),
              rfEdges: s.rfEdges.filter((e) => !tabNodeIds.has(e.source) && !tabNodeIds.has(e.target)),
            };
          });
          // 全局节点 (Transport/Protocol) 在后端全局表中归属最后提交它的 tab:
          // 先重同步全部存活 tab (把全局节点重新托管到存活 tab 名下),
          // 再移除被删 tab 的图 — 否则后端的 retain 清理会连带删掉全局节点。
          // 必须等存活 tab 同步落地后再 remove (先后顺序即正确性本身)
          const syncs = get()
            .controlTabs.map((t) => get().syncTabGraph(t.id) as Promise<unknown>);
          void Promise.all(syncs).then(() => get().removeTabGraph(tabId));
        }
      ),

    setActiveControlTab: (tabId) => set({ activeControlTabId: tabId }),

    renameControlTab: (tabId, name) =>
      withHistoryOp(
        { opKey: 'opRenameControlTab', detailText: name, target: tabTarget() },
        () =>
        set((s) => ({
          controlTabs: s.controlTabs.map((t) =>
            t.id === tabId ? { ...t, name } : t
          ),
        }))
      ),
  };
}
