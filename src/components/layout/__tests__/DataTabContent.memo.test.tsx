import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { useState, type FunctionComponent } from 'react';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { DataTabContent } from '../DataTabContent';
import { useAppStore } from '../../../store/appStore';
import { useWaveformScopeStore, createPerWidgetState } from '../../../store/waveformScopeStore';
import { PieChart } from '../../widgets/pieChart/PieChartWidget';
import type { DataTab, WidgetConfig } from '../../../types';

const STABLE_TAB: DataTab = { id: 'stable', type: 'pie', name: 'Stable', widgetId: 'pie-1', closable: true };
const HEAVY_TAB: DataTab = { id: 'heavy', type: 'can', name: 'Heavy', closable: true };

const PIE_WIDGET: WidgetConfig = {
  kind: 'PieChart',
  params: { id: 'pie-1', label: 'Pie', segments: ['A', 'B'], channels: [0, 1] },
};

/// React 19 的 memo 返回含 `.type` (真正渲染函数) 的对象 — 直接 spy `.type`
/// 即可精确统计 memo 组件内部的实际渲染次数 (memo bailout 不会调用内部函数)
function spyPieChartRender() {
  return vi.spyOn(PieChart as unknown as { type: FunctionComponent }, 'type');
}

function resetStores() {
  useAppStore.setState({
    lang: 'zh',
    dataTabs: [STABLE_TAB, HEAVY_TAB],
    widgets: [PIE_WIDGET],
    rfEdges: [],
    rfNodes: [],
    detectedChannels: {},
  });
  useWaveformScopeStore.setState({ states: { 'default-waveform': createPerWidgetState(4) } });
}

/// 模拟两个数据 Tab 内容同时挂载 (常驻兄弟视图), 切换活动 Tab 只改变可见性。
/// 活动 Tab 切换时 harness 重渲染, 兄弟 Tab 的 DataTabContent props 不变
/// (memo 短路) 且订阅的 store 未变, 其子树不应重渲染。
function SiblingTabsHarness() {
  const [active, setActive] = useState<'stable' | 'heavy'>('stable');
  return (
    <div>
      <button type="button" onClick={() => setActive(active === 'stable' ? 'heavy' : 'stable')}>
        switch
      </button>
      <div style={{ display: active === 'stable' ? undefined : 'none' }} data-testid="stable-view">
        <DataTabContent tabId="stable" />
      </div>
      <div style={{ display: active === 'heavy' ? undefined : 'none' }} data-testid="heavy-view">
        <DataTabContent tabId="heavy" />
      </div>
    </div>
  );
}

describe('DataTabContent sibling-tab isolation', () => {
  beforeAll(() => {
    // jsdom 缺失的浏览器 API — PieChart full 模式使用 ResizeObserver
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe() { return undefined; }
        unobserve() { return undefined; }
        disconnect() { return undefined; }
      }
    );
    // 固定定时器: PieChart 内部 100ms 轮询永不触发, 保证渲染计数确定性
    vi.useFakeTimers();
  });

  afterAll(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  beforeEach(() => {
    resetStores();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('does not re-render the stable sibling view when the active tab switches', () => {
    const spy = spyPieChartRender();

    render(<SiblingTabsHarness />);
    // 初始活动 Tab = stable, pie 视图渲染一次
    expect(spy).toHaveBeenCalledTimes(1);

    // 切到 heavy Tab — harness 重渲染, stable 兄弟的 DataTabContent props/订阅均未变
    fireEvent.click(screen.getByRole('button', { name: 'switch' }));
    expect(spy).toHaveBeenCalledTimes(1);

    // 切回 stable Tab — 依旧不重渲染
    fireEvent.click(screen.getByRole('button', { name: 'switch' }));
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('does not cascade DataTabContent store re-renders into the mounted view', () => {
    const spy = spyPieChartRender();

    render(<DataTabContent tabId="stable" />);
    expect(spy).toHaveBeenCalledTimes(1);

    // DataTabContent 订阅了 lang / rfEdges — 这两个 store 变化会触发其自身重渲染,
    // 但 PieTabView 分支 props 稳定 (widget 引用 / noopRemove 常量), memo 短路
    act(() => useAppStore.setState({ lang: 'en' }));
    act(() => useAppStore.setState({ rfEdges: [{ id: 'e1', source: 's', target: 't' }] }));
    expect(spy).toHaveBeenCalledTimes(1);

    // 兄弟 heavy 数据变化 (widgets 数组新增无关控件) 同样不级联
    act(() => useAppStore.setState({ widgets: [PIE_WIDGET, { kind: 'Command', params: { id: 'cmd-1', label: 'Cmd', frames: [{ id: 'f1', label: 'F1', blocks: [], appendNewline: false, sendMode: 'manual', timerMs: 100 }], loopbackEnabled: false, loopbackHistory: [] } }] }));
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('still re-renders the view when its own widget identity changes', () => {
    const spy = spyPieChartRender();

    render(<DataTabContent tabId="stable" />);
    expect(spy).toHaveBeenCalledTimes(1);

    // 替换 pie widget 对象 (同 id 新引用) → 分支 props 变化 → 应重渲染
    act(() =>
      useAppStore.setState({
        widgets: [{ ...PIE_WIDGET, params: { ...PIE_WIDGET.params, label: 'Pie v2' } }],
      })
    );
    expect(spy).toHaveBeenCalledTimes(2);
  });
});
