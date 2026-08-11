import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';

/// RawData 视图的持久化偏好 — 按 widgetId 分别保存, 无 widget 时使用 'global' key
export interface RawDataViewPrefs {
  grouping: 'grid' | 'line';
  repr: 'hex' | 'ascii';
  showTimestamp: boolean;
  showOffset: boolean;
  autoScroll: boolean;
  hexColorMode: 'none' | 'printable' | 'range';
  sendPanelMode: 'bottom' | 'separate';
  appendMode: 'none' | 'nl' | 'tab' | 'nl_tab';
}

/// 默认偏好 — 与 RawDataView 初始 useState 一致
export const DEFAULT_RAW_DATA_PREFS: RawDataViewPrefs = {
  grouping: 'grid',
  repr: 'hex',
  showTimestamp: true,
  showOffset: true,
  autoScroll: true,
  hexColorMode: 'printable',
  sendPanelMode: 'bottom',
  appendMode: 'nl',
};

interface RawDataViewStore {
  /// key: widgetId (无 widget 时为 'global')
  prefsByWidget: Record<string, RawDataViewPrefs>;
  setPrefs: (widgetId: string, prefs: RawDataViewPrefs) => void;
}

/// 每个 RawData widget 独立持久化其 UI 配置 — localStorage 持久化 (key: vofa-rawdata-view)
export const useRawDataViewStore = create<RawDataViewStore>()(
  persist(
    (set) => ({
      prefsByWidget: {},
      setPrefs: (widgetId, prefs) =>
        set((s) => ({ prefsByWidget: { ...s.prefsByWidget, [widgetId]: prefs } })),
    }),
    {
      name: 'vofa-rawdata-view',
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => ({ prefsByWidget: s.prefsByWidget }),
    }
  )
);

/// 读取指定 widget 的偏好, 未持久化过时返回默认值
export function getRawDataViewPrefs(widgetId: string): RawDataViewPrefs {
  return useRawDataViewStore.getState().prefsByWidget[widgetId] ?? DEFAULT_RAW_DATA_PREFS;
}

/// 全部 widget 的偏好 (供未来导出使用)
export function getAllRawDataViewPrefs(): Record<string, RawDataViewPrefs> {
  return useRawDataViewStore.getState().prefsByWidget;
}
