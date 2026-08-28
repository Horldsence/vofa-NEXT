import { beforeEach, describe, expect, it, vi } from 'vitest';

// persist 中间件需要 localStorage — 在导入 store 前提供内存桩
// (Node 的实验性 localStorage 需 --localstorage-file, jsdom 环境下不可用)
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

import { useLayoutStore } from '../layoutStore';

/// AI 面板布局 — 停靠位置 / 浮动矩形 / 持久化字段

describe('layoutStore AI 面板布局', () => {
  beforeEach(() => {
    useLayoutStore.setState({
      aiPanelVisible: false,
      aiDock: 'right',
      aiFloatRect: { x: 220, y: 120, w: 400, h: 480 },
      draggingAiPanel: false,
      aiDockEdgeHover: null,
    });
  });

  it('dropAiToFloat 以落点为标题栏位置并 clamp 到窗口内', () => {
    useLayoutStore.getState().dropAiToFloat(-500, 999999);

    const st = useLayoutStore.getState();
    expect(st.aiDock).toBe('float');
    expect(st.aiFloatRect.x).toBeGreaterThanOrEqual(8);
    expect(st.aiFloatRect.y).toBeLessThanOrEqual(window.innerHeight - st.aiFloatRect.h - 8);
    // 尺寸不变, 只调位置
    expect(st.aiFloatRect.w).toBe(400);
    expect(st.aiFloatRect.h).toBe(480);
  });

  it('停靠位置与可见性持久化到 localStorage (vofa-layout)', () => {
    useLayoutStore.getState().setAiDock('bottom');
    useLayoutStore.getState().setAiPanelVisible(true);

    const saved = JSON.parse(localStorage.getItem('vofa-layout') ?? '{}') as {
      state?: { aiDock?: string; aiPanelVisible?: boolean };
    };
    expect(saved.state?.aiDock).toBe('bottom');
    expect(saved.state?.aiPanelVisible).toBe(true);
  });

  it('拖拽瞬态字段不持久化 (partialize 只保留布局)', () => {
    useLayoutStore.getState().setDraggingAiPanel(true);
    useLayoutStore.getState().setAiDockEdgeHover('left');

    const saved = JSON.parse(localStorage.getItem('vofa-layout') ?? '{}') as {
      state?: Record<string, unknown>;
    };
    expect(saved.state?.draggingAiPanel).toBeUndefined();
    expect(saved.state?.aiDockEdgeHover).toBeUndefined();
  });
});
