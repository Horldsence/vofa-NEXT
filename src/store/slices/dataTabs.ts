import { t } from '../../i18n';
import type { DataTab, WidgetConfig } from '../../types';
import { useAppStore } from '../appStore';
import { widgetToTab } from '../../lib/utils/widgetTab';
import type { AppSlice } from './types';

export interface DataTabSlice {
  dataTabs: DataTab[];
  activeDataTabId: string;
  addDataTab: (tab: DataTab) => void;
  removeDataTab: (tabId: string) => void;
  setActiveDataTab: (tabId: string) => void;
  addCanTab: () => void;
  addLogicTab: () => void;
  /// 整库唯一 — 首次创建后再次调用会切到已存在 tab,
  /// 新建 tab 由 DockLayout useEffect → reconcile 自动安置到 focusedCard.tabIds
  addCompileErrorsTab: () => void;
  /// 新增 — 编译结果面板 (连接列表)
  addCompileResultsTab: () => void;
  /// 新增 — 操作历史面板 (撤销/重做记录, 恒定单例)
  addOperationHistoryTab: () => void;
  /// 新增 — 由控件 id 构造对应窗口 Tab (已存在则激活, 否则新建并激活)
  addWidgetTab: (widget: WidgetConfig) => void;
}

export const createDataTabSlice: AppSlice<DataTabSlice> = (set, get) => {
  return {
    dataTabs: [
      { id: 'compile-errors-fixed', type: 'compile-errors', name: 'Compile Errors', closable: false },
      { id: 'compile-results-fixed', type: 'compile-results', name: 'Compile Results', closable: false },
    ],
    activeDataTabId: 'compile-results-fixed',

    addDataTab: (tab) =>
      set((s) => ({
        dataTabs: [...s.dataTabs, tab],
        activeDataTabId: tab.id,
      })),

    removeDataTab: (tabId) =>
      set((s) => {
        const tab = s.dataTabs.find((t: DataTab) => t.id === tabId);
        if (!tab?.closable) return s;
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

    addCompileErrorsTab: () => {
      const existing = get().dataTabs.find((t: DataTab) => t.type === 'compile-errors');
      if (existing) {
        set({ activeDataTabId: existing.id });
        return;
      }
      const tab: DataTab = {
        id: `compile-errors-${Date.now()}`,
        type: 'compile-errors',
        name: t(get().lang, 'compileErrorsTitle'),
        closable: true,
      };
      set({
        dataTabs: [...get().dataTabs, tab],
        activeDataTabId: tab.id,
      });
      // dockStore 的安置由 DockLayout useEffect → reconcile 自动处理
      // (reconcile 把 missing tabId 推入 focusedCard.tabIds 或首个 data card)
    },

    addCompileResultsTab: () => {
      // 整库已固定存在 compile-results-fixed; 此 action 用于兼容后续版本可能开放的
      // 多个独立结果实例, 也允许在 fixed Tab 不可用时兜底新建
      const existingFixed = get().dataTabs.find((t: DataTab) => t.id === 'compile-results-fixed');
      if (existingFixed) {
        set({ activeDataTabId: existingFixed.id });
        return;
      }
      const tab: DataTab = {
        id: 'compile-results-fixed',
        type: 'compile-results',
        name: t(get().lang, 'connectionResultsTitle'),
        closable: false,
      };
      set({
        dataTabs: [...get().dataTabs, tab],
        activeDataTabId: tab.id,
      });
    },

    addOperationHistoryTab: () => {
      const existing = get().dataTabs.find((t: DataTab) => t.type === 'operation-history');
      if (existing) {
        set({ activeDataTabId: existing.id });
        return;
      }
      const tab: DataTab = {
        id: `operation-history-${Date.now()}`,
        type: 'operation-history',
        name: t(get().lang, 'operationHistoryTitle'),
        closable: true,
      };
      set({
        dataTabs: [...get().dataTabs, tab],
        activeDataTabId: tab.id,
      });
    },

    addWidgetTab: (widget) => {
      const tab = widgetToTab(widget);
      if (!tab) return;
      const existing = get().dataTabs.find((t: DataTab) => t.id === tab.id);
      if (existing) {
        set({ activeDataTabId: existing.id });
        return;
      }
      set({
        dataTabs: [...get().dataTabs, tab],
        activeDataTabId: tab.id,
      });
    },
  };
}

// ============================================================
// 数据面板菜单条目的「单一事实源」
// ============================================================
//
// 菜单栏 (Windows MenuBar / 原生 menu_shell) 与侧边栏 (QuickStart / WidgetPalette)
// 三个入口共用同一份条目定义, 派生面板的 available 计算 (画布是否有该类 widget)
// 也只此一处。条目结构:
//   { type, labelKey, available, open }
//
// 独立面板 (compile-errors / compile-results / can / logic) 无需 widget,
// 总是 available; 派生面板按 widgets.some(...) 决定是否可点。

import type { AppStore } from '../appStore';

export interface DataPanelEntry {
  /// DataTabType 字符串 — 仅作图标 / 类型查询
  type: DataTab['type'];
  /// i18n key (en.yml / zh.yml 同名)
  labelKey: string;
  /// 始终可用 (独立面板) / 按 widget 存在动态可用 (派生面板)
  available: boolean;
  /// 触发打开的副作用 — 调用 store action + 切到目标 Tab 所在 Dock 卡片
  open: () => void;
  /// 分组 — 'standalone' = 独立面板, 'derived' = 由 widget 派生
  group: 'standalone' | 'derived';
}

export function getAvailableDataPanelEntries(
  state: Pick<AppStore, 'dataTabs' | 'widgets' | 'lang'>,
  actions: Pick<AppStore, 'addCompileErrorsTab' | 'addCompileResultsTab' | 'addCanTab' | 'addLogicTab' | 'addOperationHistoryTab' | 'addDataTab' | 'setActiveDataTab' | 'addWidgetTab'>,
): DataPanelEntry[] {
  const widgetKinds = new Set(state.widgets.map((w) => w.kind));
  // 同一 kind 可有多个 widget (例如多个 Waveform) — 取首个派生 Tab 用作打开目标
  const firstWidgetOf = (kind: WidgetConfig['kind']) =>
    state.widgets.find((w) => w.kind === kind) ?? null;

  const standalone: DataPanelEntry[] = [
    {
      type: 'compile-errors',
      labelKey: 'menuPanelOpenCompileErrors',
      available: true,
      open: () => {
        actions.addCompileErrorsTab();
      },
      group: 'standalone',
    },
    {
      type: 'compile-results',
      labelKey: 'menuPanelOpenCompileResults',
      available: true,
      open: () => {
        actions.addCompileResultsTab();
      },
      group: 'standalone',
    },
    {
      type: 'can',
      labelKey: 'menuPanelOpenCan',
      available: true,
      open: () => {
        actions.addCanTab();
      },
      group: 'standalone',
    },
    {
      type: 'logic',
      labelKey: 'menuPanelOpenLogic',
      available: true,
      open: () => {
        actions.addLogicTab();
      },
      group: 'standalone',
    },
    {
      type: 'operation-history',
      labelKey: 'menuPanelOpenOperationHistory',
      available: true,
      open: () => {
        actions.addOperationHistoryTab();
      },
      group: 'standalone',
    },
  ];

  const derivedDefs: {
    type: DataTab['type'];
    labelKey: string;
    kind: WidgetConfig['kind'];
  }[] = [
    { type: 'waveform-extra', labelKey: 'dataTabWaveform', kind: 'Waveform' },
    { type: 'spectrum', labelKey: 'dataTabSpectrum', kind: 'Spectrum' },
    { type: 'raw', labelKey: 'dataTabRawData', kind: 'RawData' },
    { type: 'pie', labelKey: 'dataTabPie', kind: 'PieChart' },
    { type: 'image', labelKey: 'dataTabImage', kind: 'Image' },
    { type: 'model3d', labelKey: 'dataTabModel3d', kind: 'Model3D' },
    { type: 'command', labelKey: 'dataTabCommand', kind: 'Command' },
    { type: 'frame-decoder', labelKey: 'dataTabFrameDecoder', kind: 'FrameDecoder' },
    { type: 'trigger', labelKey: 'dataTabTrigger', kind: 'Trigger' },
  ];

  const derived: DataPanelEntry[] = derivedDefs.map((d) => {
    const widget = firstWidgetOf(d.kind);
    return {
      type: d.type,
      labelKey: d.labelKey,
      available: widgetKinds.has(d.kind) && widget != null,
      open: () => {
        if (!widget) return;
        actions.addWidgetTab(widget);
      },
      group: 'derived',
    };
  });

  return [...standalone, ...derived];
}

// ============================================================
// 数据面板 Tab 轮转
// ============================================================
//
// 在同一 DataTabType 的多个 Tab 间循环跳 (Waveform / Spectrum / RawData / ...
// 同类控件可有多个, 因此派生面板常有 > 1 个 Tab)。
// - 已有 ≥ 2 个匹配 Tab: 当前 active 是其一 → 下一个; 否则第一个
// - 仅 1 个匹配: 激活它
// - 0 个匹配: 创建并激活
//   - 独立类型走对应 add action (注意 can / logic 无固定初始 Tab, 首次
//     打开必须靠这里兜底创建; compile-errors / results 才有 fixed 初始 Tab)
//   - 派生类型找到画布首个同类 widget, addWidgetTab 创建并激活

export function cycleDataPanelTab(type: DataTab['type']): void {
  const app = useAppStore.getState();
  const matching = app.dataTabs.filter((tab) => tab.type === type);
  if (matching.length === 0) {
    // 独立类型: 对应 add action (已存在则切过去, 不存在则创建)
    const opener = standaloneOpener(type);
    if (opener) {
      opener(app);
      return;
    }
    // 派生类型: 用首个同类 widget 创建
    const widget = app.widgets.find((w) => widgetTabTypeOfKind(w.kind) === type);
    if (widget) app.addWidgetTab(widget);
    return;
  }
  // 多个匹配: 取当前 active 的索引, +1, 越界回到 0
  const currentIdx = matching.findIndex((t) => t.id === app.activeDataTabId);
  const next = matching[(currentIdx + 1) % matching.length];
  app.setActiveDataTab(next.id);
}

/// 独立面板类型 → 所在 slice 的创建 action (cycleDataPanelTab 兜底用)
function standaloneOpener(type: DataTab['type']): ((app: DataTabSlice) => void) | null {
  switch (type) {
    case 'compile-errors':
      return (app) => app.addCompileErrorsTab();
    case 'compile-results':
      return (app) => app.addCompileResultsTab();
    case 'can':
      return (app) => app.addCanTab();
    case 'logic':
      return (app) => app.addLogicTab();
    case 'operation-history':
      return (app) => app.addOperationHistoryTab();
    default:
      return null;
  }
}

/// widget kind → 对应 DataTab type 的辅助 (避免外部引用 widgetTab 模块时的循环耦合)
function widgetTabTypeOfKind(kind: WidgetConfig['kind']): DataTab['type'] | null {
  switch (kind) {
    case 'Waveform':
      return 'waveform-extra';
    case 'PieChart':
      return 'pie';
    case 'Image':
      return 'image';
    case 'Model3D':
      return 'model3d';
    case 'Spectrum':
      return 'spectrum';
    case 'Command':
      return 'command';
    case 'FrameDecoder':
      return 'frame-decoder';
    case 'RawData':
      return 'raw';
    case 'Trigger':
      return 'trigger';
    default:
      return null;
  }
}
