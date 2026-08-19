import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { createElement } from 'react';

// dockStore/layoutStore 的 persist 中间件需要 localStorage — 在导入 store 前提供内存桩
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

import { dockDrag, __resetForTests } from '../../../lib/dockDrag';
import { DockDragGhost } from '../DockDragGhost';

function moveTo(x: number, y: number) {
  window.dispatchEvent(new MouseEvent('pointermove', { clientX: x, clientY: y, button: 0 }));
}

describe('DockDragGhost 渲染', () => {
  let container: HTMLDivElement;

  beforeEach(() => {
    __resetForTests();
    document.body.innerHTML = '';
    container = document.createElement('div');
    document.body.appendChild(container);
    document.elementFromPoint = () => null;
    act(() => {
      createRoot(container).render(createElement(DockDragGhost));
    });
  });

  it('拖拽激活后幽灵显示, 且以指针坐标定位', () => {
    expect(container.querySelector('[aria-hidden]')).toBeNull();
    act(() => {
      dockDrag.begin({ clientX: 10, clientY: 10, button: 0 }, { kind: 'sidebar', label: 'Sidebar' });
      moveTo(100, 100);
    });
    const ghost = container.querySelector('[aria-hidden]') as HTMLElement | null;
    expect(ghost).not.toBeNull();
    expect(ghost!.style.left).toBe('100px');
    expect(ghost!.style.top).toBe('100px');
  });
});
