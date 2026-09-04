import { beforeEach, describe, expect, it, vi } from 'vitest';

// persist 中间件 localStorage 桩 (与其他 store 测试一致)
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

import { tauriMock } from '../../test/setup';
import { useAppStore } from '../appStore';
import { useDockStore } from '../dockStore';
import { NODE_PROPERTIES_TAB_ID } from '../../types';
import type { DataTab } from '../../types';

function resetStores(dataTabs: DataTab[]): void {
  tauriMock.invoke.mockClear();
  (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue(
    undefined
  );
  useAppStore.setState({ dataTabs, activeDataTabId: dataTabs[0]?.id ?? '' } as never);
  // 重置 dock 布局为默认 (persist 桩为空, setState 直接覆盖)
  useDockStore.setState({
    root: structuredClone(useDockStore.getState().root),
    cards: { ...useDockStore.getState().cards },
    focusedCardId: null,
  });
}

describe('节点属性面板 tab (单例)', () => {
  beforeEach(() => {
    resetStores([
      { id: 'compile-results-fixed', type: 'compile-results', name: 'Compile Results', closable: false },
      { id: NODE_PROPERTIES_TAB_ID, type: 'node-properties', name: 'Properties', closable: true },
    ]);
  });

  it('初始已存在 → 调用仅激活, 不重建', () => {
    useAppStore.getState().addNodePropertiesTab();
    const tabs = useAppStore.getState().dataTabs.filter((t) => t.type === 'node-properties');
    expect(tabs).toHaveLength(1);
    expect(tabs[0].id).toBe(NODE_PROPERTIES_TAB_ID);
    expect(useAppStore.getState().activeDataTabId).toBe(NODE_PROPERTIES_TAB_ID);
  });

  it('关闭后调用 → 以同一稳定 id 重建 (默认布局专属卡按 id 承载)', () => {
    useAppStore.getState().removeDataTab(NODE_PROPERTIES_TAB_ID);
    expect(
      useAppStore.getState().dataTabs.some((t) => t.type === 'node-properties')
    ).toBe(false);

    useAppStore.getState().addNodePropertiesTab();
    const tabs = useAppStore.getState().dataTabs.filter((t) => t.type === 'node-properties');
    expect(tabs).toHaveLength(1);
    expect(tabs[0].id).toBe(NODE_PROPERTIES_TAB_ID);
  });
});

describe('dockPropertiesTab (属性面板右侧停靠)', () => {
  beforeEach(() => {
    resetStores([
      { id: 'compile-results-fixed', type: 'compile-results', name: 'Compile Results', closable: false },
      { id: NODE_PROPERTIES_TAB_ID, type: 'node-properties', name: 'Properties', closable: true },
    ]);
  });

  it('tab 未被任何卡片承载 → 在画布卡右侧拆分较小的新卡', () => {
    useDockStore.getState().dockPropertiesTab(NODE_PROPERTIES_TAB_ID);

    const dock = useDockStore.getState();
    const host = Object.values(dock.cards).find(
      (c) => c.kind === 'data' && c.tabIds.includes(NODE_PROPERTIES_TAB_ID)
    );
    expect(host).toBeDefined();
    expect(host!.tabIds).toEqual([NODE_PROPERTIES_TAB_ID]);

    // 新卡与画布卡同属一个 row split, 且份额较小 (< 画布份额)
    const controlCard = Object.values(dock.cards).find((c) => c.kind === 'control')!;
    const findSiblingSplit = (node: typeof dock.root): null | { dir: string; sizes: number[]; children: { type: string; cardId?: string }[] } => {
      if (node.type === 'card') return null;
      const cardIds = node.children.map((c) => (c.type === 'card' ? c.cardId : undefined));
      if (
        node.dir === 'row' &&
        cardIds.includes(controlCard.id) &&
        cardIds.includes(host!.id)
      ) {
        return { dir: node.dir, sizes: node.sizes, children: node.children };
      }
      for (const child of node.children) {
        const hit = findSiblingSplit(child);
        if (hit) return hit;
      }
      return null;
    };
    const split = findSiblingSplit(dock.root);
    expect(split).not.toBeNull();
    const controlIdx = split!.children.findIndex(
      (c) => c.type === 'card' && c.cardId === controlCard.id
    );
    const propsIdx = split!.children.findIndex(
      (c) => c.type === 'card' && c.cardId === host!.id
    );
    expect(propsIdx).toBe(controlIdx + 1); // 画布在左, 属性在右
    expect(split!.sizes[propsIdx]).toBeLessThan(split!.sizes[controlIdx]);
  });

  it('tab 已有归属 → no-op (尊重用户手动安排)', () => {
    const dock = useDockStore.getState();
    const dataCard = Object.values(dock.cards).find((c) => c.kind === 'data' && c.id !== 'properties-main');
    // 把 tab 手动塞进数据卡 (模拟用户拖动合并)
    useDockStore.setState({
      cards: {
        ...dock.cards,
        [dataCard!.id]: { ...dataCard!, tabIds: [...dataCard!.tabIds, NODE_PROPERTIES_TAB_ID] },
      },
    });

    const before = useDockStore.getState();
    useDockStore.getState().dockPropertiesTab(NODE_PROPERTIES_TAB_ID);
    const after = useDockStore.getState();
    expect(after.cards).toBe(before.cards);
    expect(after.root).toBe(before.root);
  });
});
