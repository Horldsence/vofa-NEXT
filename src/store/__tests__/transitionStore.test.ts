import { beforeEach, describe, expect, it, vi } from 'vitest';

// 捕获 startTransition 回调, 验证 store 动作确实经由 startTransition 延迟执行
vi.mock('react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('react')>();
  return { ...actual, startTransition: vi.fn() };
});

import { startTransition } from 'react';
import { transitionStore } from '../../lib/utils/transitionStore';
import { useAppStore } from '../appStore';

describe('transitionStore', () => {
  beforeEach(() => {
    vi.mocked(startTransition).mockReset();
    useAppStore.setState({
      dataTabs: [
        { id: 'waveform-fixed', type: 'waveform', name: 'Waveform', closable: false },
        { id: 'can-tab', type: 'can', name: 'CAN', closable: true },
      ],
      activeDataTabId: 'waveform-fixed',
    });
  });

  it('routes the tab-switch store action through startTransition and defers the update', () => {
    transitionStore(() => useAppStore.getState().setActiveDataTab('can-tab'));

    // 动作未被内联执行 — 而是交给 startTransition 调度
    expect(startTransition).toHaveBeenCalledTimes(1);
    const transitionScope = vi.mocked(startTransition).mock.calls[0][0];

    // transition 作用域执行前, 状态不变 (渲染被延迟, 不阻塞当前帧)
    expect(useAppStore.getState().activeDataTabId).toBe('waveform-fixed');

    // 作用域执行后状态生效
    transitionScope();
    expect(useAppStore.getState().activeDataTabId).toBe('can-tab');
  });

  it('returns synchronously and does not throw for control-tab and sidebar actions', () => {
    expect(() =>
      transitionStore(() => useAppStore.getState().setActiveControlTab('default'))
    ).not.toThrow();
    expect(() => transitionStore(() => useAppStore.getState().toggleSidebar('protocol'))).not.toThrow();
    expect(startTransition).toHaveBeenCalledTimes(2);

    // 执行被捕获的 transition 作用域, 确认侧边栏动作仍正常生效
    vi.mocked(startTransition).mock.calls[1][0]();
    expect(useAppStore.getState().sidebarView).toBe('protocol');
    expect(useAppStore.getState().sidebarVisible).toBe(true);
  });
});
