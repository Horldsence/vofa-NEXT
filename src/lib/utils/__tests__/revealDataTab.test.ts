import { describe, expect, it, vi, beforeEach } from 'vitest';

// 该 vitest jsdom 环境未启用 localStorage — dockStore 的 persist
// 中间件在 setState 时会写入 storage, 需在导入 store 前提供内存桩
vi.hoisted(() => {
  const store = new Map<string, string>();
  const localStorageMock = {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
    clear: () => store.clear(),
    key: (index: number) => [...store.keys()][index] ?? null,
    get length() {
      return store.size;
    },
  };
  const g = globalThis as { localStorage?: unknown };
  g.localStorage = localStorageMock;
});

import { useAppStore } from '../../../store/appStore';
import { useDockStore } from '../../../store/dockStore';
import { openDataPanelAndReveal } from '../revealDataTab';
import { cycleDataPanelTab } from '../../../store/slices/dataTabs';

/// 回归背景: 数据面板入口曾只调 setFocusedCard 聚焦卡片而不切卡片的
/// activeTabId, 造成「点了没反应」的假跳转。此处在 store 层直接断言
/// openDataPanelAndReveal 链路的完整副作用。
describe('openDataPanelAndReveal', () => {
  beforeEach(() => {
    useDockStore.setState({
      cards: {
        'control-main': {
          id: 'control-main',
          kind: 'control',
          tabIds: ['ctl-1'],
          activeTabId: 'ctl-1',
        },
        'data-a': {
          id: 'data-a',
          kind: 'data',
          tabIds: ['wave-1', 'wave-2'],
          activeTabId: 'wave-1',
        },
        'data-b': {
          id: 'data-b',
          kind: 'data',
          tabIds: ['can-1'],
          activeTabId: 'can-1',
        },
      },
      focusedCardId: 'control-main',
    });

    useAppStore.setState({
      dataTabs: [
        { id: 'wave-1', type: 'waveform-extra', name: 'Waveform 1', closable: true },
        { id: 'wave-2', type: 'waveform-extra', name: 'Waveform 2', closable: true },
        { id: 'can-1', type: 'can', name: 'CAN Frames', closable: true },
      ],
      activeDataTabId: 'wave-1',
    });
  });

  it('切到目标 Tab 所在卡片并聚焦, 全局镜像同步', () => {
    openDataPanelAndReveal(() => useAppStore.getState().setActiveDataTab('wave-2'));

    const dock = useDockStore.getState();
    expect(dock.cards['data-a'].activeTabId).toBe('wave-2');
    expect(dock.focusedCardId).toBe('data-a');
    expect(useAppStore.getState().activeDataTabId).toBe('wave-2');
    // 不波及其他卡片
    expect(dock.cards['data-b'].activeTabId).toBe('can-1');
    expect(dock.cards['control-main'].activeTabId).toBe('ctl-1');
  });

  it('重复触发在同类型多 Tab 间轮转且每次都切到目标卡', () => {
    openDataPanelAndReveal(() => cycleDataPanelTab('waveform-extra'));
    expect(useAppStore.getState().activeDataTabId).toBe('wave-2');

    openDataPanelAndReveal(() => cycleDataPanelTab('waveform-extra'));
    expect(useAppStore.getState().activeDataTabId).toBe('wave-1');

    const dock = useDockStore.getState();
    expect(dock.cards['data-a'].activeTabId).toBe('wave-1');
    expect(dock.focusedCardId).toBe('data-a');
  });

  it('目标 Tab 尚未安置到任何卡片时不产生 dock 副作用', () => {
    const dockBefore = useDockStore.getState();

    // 模拟新建 Tab 刚写入 appStore、reconcile 尚未安置的瞬间
    openDataPanelAndReveal(() => useAppStore.getState().setActiveDataTab('brand-new'));

    const dockAfter = useDockStore.getState();
    expect(dockAfter.cards).toEqual(dockBefore.cards);
    expect(dockAfter.focusedCardId).toBe(dockBefore.focusedCardId);
    // appStore 镜像由 trigger 本身写入, 不回滚
    expect(useAppStore.getState().activeDataTabId).toBe('brand-new');
  });
});

/// 回归背景: can / logic 无固定初始 Tab, cycleDataPanelTab 的 0 匹配分支此前
/// 只会走派生 widget 创建, 对这两个类型静默无操作 —「打开CAN帧 / 逻辑分析仪」
/// 点了没反应。
describe('openDataPanelAndReveal + cycleDataPanelTab 独立面板兜底', () => {
  beforeEach(() => {
    // 全新会话基线: 仅预置两个 fixed 编译 Tab (与 appStore 初始状态一致)
    useDockStore.setState({
      cards: {
        'data-main': {
          id: 'data-main',
          kind: 'data',
          tabIds: ['compile-errors-fixed'],
          activeTabId: 'compile-errors-fixed',
        },
      },
      focusedCardId: null,
    });
    useAppStore.setState({
      dataTabs: [
        { id: 'compile-errors-fixed', type: 'compile-errors', name: 'Compile Errors', closable: false },
        { id: 'compile-results-fixed', type: 'compile-results', name: 'Compile Results', closable: false },
      ],
      activeDataTabId: 'compile-results-fixed',
    });
  });

  it('无 CAN Tab 时首次点击创建并激活', () => {
    openDataPanelAndReveal(() => cycleDataPanelTab('can'));

    const app = useAppStore.getState();
    const canTabs = app.dataTabs.filter((t) => t.type === 'can');
    expect(canTabs).toHaveLength(1);
    expect(app.activeDataTabId).toBe(canTabs[0].id);
  });

  it('无 Logic Tab 时首次点击同样可创建', () => {
    openDataPanelAndReveal(() => cycleDataPanelTab('logic'));

    const logicTabs = useAppStore.getState().dataTabs.filter((t) => t.type === 'logic');
    expect(logicTabs).toHaveLength(1);
    expect(useAppStore.getState().activeDataTabId).toBe(logicTabs[0].id);
  });

  it('Tab 已安置后再次点击只激活, 不重复创建', () => {
    openDataPanelAndReveal(() => cycleDataPanelTab('can'));
    const createdId = useAppStore.getState().activeDataTabId;

    // 模拟 DockLayout reconcile 把新 Tab 安置进卡片
    useDockStore.setState((s) => {
      const card = s.cards['data-main'];
      return {
        cards: {
          ...s.cards,
          'data-main': { ...card, tabIds: [...card.tabIds, createdId], activeTabId: createdId },
        },
      };
    });

    openDataPanelAndReveal(() => cycleDataPanelTab('can'));

    expect(useAppStore.getState().dataTabs.filter((t) => t.type === 'can')).toHaveLength(1);
    expect(useAppStore.getState().activeDataTabId).toBe(createdId);
    // 跳转把卡片可视 Tab 切过去
    expect(useDockStore.getState().cards['data-main'].activeTabId).toBe(createdId);
  });
});
