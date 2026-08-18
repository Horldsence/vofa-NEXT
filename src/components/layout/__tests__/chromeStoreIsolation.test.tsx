import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import { memo, type FunctionComponent } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { useAppStore, type AppStore } from '../../../store/appStore';
import { useDockStore } from '../../../store/dockStore';
import { Sidebar } from '../Sidebar';
import { StatusBar } from '../StatusBar';
import { ActivityBar } from '../ActivityBar';

// 该 vitest jsdom 环境未启用 localStorage — dockStore/layoutStore 的 persist
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

/// 验证 chrome 组件的 store 订阅粒度: 更新 data slice (rawDataVersion)
/// 不得重渲染只订阅其他 slice 的组件。
/// React 19 的 memo 返回含 `.type` (真正渲染函数) 的对象 — spy `.type`
/// 即可精确统计 memo 组件内部的实际渲染次数。

/// 复刻 DockCardFrame 的按 cardId 窄化 selector — 验证跨卡片 Tab 变化的渲染隔离
const CardTabsProbe = memo(function CardTabsProbe({ cardId }: { cardId: string }) {
  const card = useDockStore((s) => s.cards[cardId]);
  const kind = card?.kind ?? 'data';
  const cardTabIds = card?.tabIds ?? [];
  const tabs: Array<{ id: string; name: string; type?: string; closable?: boolean }> = useAppStore(
    useShallow(
      ((s: AppStore): Array<{ id: string; name: string; type?: string; closable?: boolean }> =>
        kind === 'control'
          ? s.controlTabs.filter((tab) => cardTabIds.includes(tab.id))
          : s.dataTabs.filter((tab) => cardTabIds.includes(tab.id)))
    )
  );
  return <span data-testid={`probe-${cardId}`}>{tabs.map((tab) => tab.name).join('|')}</span>;
});

function spyMemoRender(component: { type: FunctionComponent }) {
  return vi.spyOn(component, 'type');
}

// 模块级初始状态快照 — 供 afterEach 恢复 (vitest 每个测试文件独立模块实例)
const INITIAL_CONTROL_TABS = useAppStore.getState().controlTabs;
const INITIAL_CARDS = useDockStore.getState().cards;
const INITIAL_LANG = useAppStore.getState().lang;
const INITIAL_RAW_VERSION = useAppStore.getState().rawDataVersion;

describe('chrome store subscription granularity', () => {
  let version = 0;
  const bumpDataVersion = () => {
    version += 1;
    act(() => useAppStore.setState({ rawDataVersion: version }));
  };

  afterEach(() => {
    vi.restoreAllMocks();
    act(() => {
      useAppStore.setState({
        controlTabs: INITIAL_CONTROL_TABS,
        lang: INITIAL_LANG,
        rawDataVersion: INITIAL_RAW_VERSION,
      });
      useDockStore.setState({ cards: INITIAL_CARDS });
    });
  });

  describe('Sidebar (订阅 sidebar slice)', () => {
    it('data slice 更新不重渲染', () => {
      const spy = spyMemoRender(Sidebar as unknown as { type: FunctionComponent });
      render(<Sidebar view="widgets" />);
      expect(spy).toHaveBeenCalledTimes(1);

      bumpDataVersion();
      bumpDataVersion();

      expect(spy).toHaveBeenCalledTimes(1);
    });

    it('自身 slice (lang) 更新时重渲染', () => {
      const spy = spyMemoRender(Sidebar as unknown as { type: FunctionComponent });
      render(<Sidebar view="widgets" />);
      expect(spy).toHaveBeenCalledTimes(1);

      act(() => useAppStore.setState({ lang: 'en' }));

      expect(spy).toHaveBeenCalledTimes(2);
    });
  });

  describe('StatusBar (订阅 connection/protocol slice)', () => {
    it('data slice 更新不重渲染', () => {
      const spy = spyMemoRender(StatusBar as unknown as { type: FunctionComponent });
      render(<StatusBar />);
      expect(spy).toHaveBeenCalledTimes(1);

      bumpDataVersion();

      expect(spy).toHaveBeenCalledTimes(1);
    });

    it('自身 slice (nodeStats) 更新时重渲染', () => {
      const spy = spyMemoRender(StatusBar as unknown as { type: FunctionComponent });
      render(<StatusBar />);
      expect(spy).toHaveBeenCalledTimes(1);

      act(() =>
        useAppStore.setState({
          nodeStats: {
            'transport-1': {
              rx_bytes: 12345, tx_bytes: 0, rx_frames: 0, tx_frames: 0,
              rx_dropped: 0, rxDroppedWindow: 0, rxDroppedTotal: 0,
            },
          },
        })
      );

      expect(spy).toHaveBeenCalledTimes(2);
    });
  });

  describe('ActivityBar (订阅 sidebar/settings/onboarding slice)', () => {
    it('data slice 更新不重渲染', () => {
      const spy = spyMemoRender(ActivityBar as unknown as { type: FunctionComponent });
      render(<ActivityBar activeView="widgets" onSelect={() => {}} />);
      expect(spy).toHaveBeenCalledTimes(1);

      bumpDataVersion();

      expect(spy).toHaveBeenCalledTimes(1);
    });

    it('自身 slice (lang) 更新时重渲染', () => {
      const spy = spyMemoRender(ActivityBar as unknown as { type: FunctionComponent });
      render(<ActivityBar activeView="widgets" onSelect={() => {}} />);
      expect(spy).toHaveBeenCalledTimes(1);

      act(() => useAppStore.setState({ lang: 'en' }));

      expect(spy).toHaveBeenCalledTimes(2);
    });
  });

  describe('DockCardFrame 按 cardId 窄化 selector (小型组合)', () => {
    beforeEach(() => {
      act(() => {
        useAppStore.setState({
          controlTabs: [
            { id: 'tab-a', name: 'Tab A', widgets: [] },
            { id: 'tab-b', name: 'Tab B', widgets: [] },
          ],
        });
        useDockStore.setState({
          cards: {
            'card-1': { id: 'card-1', kind: 'control', tabIds: ['tab-a'], activeTabId: 'tab-a' },
            'card-2': { id: 'card-2', kind: 'control', tabIds: ['tab-b'], activeTabId: 'tab-b' },
          },
        });
      });
    });

    it('重命名卡片 2 的 Tab 不重渲染卡片 1', () => {
      const spy = spyMemoRender(CardTabsProbe as unknown as { type: FunctionComponent });
      render(
        <>
          <CardTabsProbe cardId="card-1" />
          <CardTabsProbe cardId="card-2" />
        </>
      );
      expect(spy).toHaveBeenCalledTimes(2);

      act(() => useAppStore.getState().renameControlTab('tab-b', 'Tab B2'));

      expect(spy).toHaveBeenCalledTimes(3);
      expect(screen.getByTestId('probe-card-1')).toHaveTextContent('Tab A');
      expect(screen.getByTestId('probe-card-2')).toHaveTextContent('Tab B2');
    });

    it('data slice 更新不重渲染 control 卡片探针', () => {
      const spy = spyMemoRender(CardTabsProbe as unknown as { type: FunctionComponent });
      render(
        <>
          <CardTabsProbe cardId="card-1" />
          <CardTabsProbe cardId="card-2" />
        </>
      );
      expect(spy).toHaveBeenCalledTimes(2);

      bumpDataVersion();

      expect(spy).toHaveBeenCalledTimes(2);
    });
  });
});
