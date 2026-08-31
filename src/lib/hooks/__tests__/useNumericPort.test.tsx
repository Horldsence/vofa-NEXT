import { describe, expect, it, beforeEach } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { useAppStore } from '../../../store/appStore';
import { useNumericInput, useNumericInputs } from '../useNumericPort';
import { tauriMock } from '../../../test/setup';

/// 复现 "Maximum update depth exceeded" 回归:
/// store facade 若每次渲染新建, useSyncExternalStore 会在每次渲染后退订重订阅,
/// dataClient.start() 又同步替换快照 → 渲染死循环。修复后 facade 按 key 缓存,
/// 任意次数的重渲染都不得产生新的 subscribe_data 订阅。

function makeGraph(widgetId: string) {
  return {
    rfNodes: [{ id: 'proto-1', type: 'protocol', position: { x: 0, y: 0 }, data: { global: true } }],
    rfEdges: [
      { id: 'e1', source: 'proto-1', sourceHandle: 'ch0', target: widgetId, targetHandle: 'value' },
    ],
  };
}

function subscribeCallCount() {
  return (tauriMock.invoke.mock.calls as unknown as [string][]).filter(
    ([command]) => command === 'subscribe_data',
  ).length;
}

beforeEach(() => {
  tauriMock.invoke.mockReset();
  tauriMock.invoke.mockResolvedValue(undefined);
});

describe('useNumericPort subscription stability', () => {
  it('useNumericInput keeps the port subscription across re-renders', () => {
    useAppStore.setState(makeGraph('w-single'));
    function Harness() {
      const [, setTick] = useState(0);
      const input = useNumericInput('w-single', 'value');
      return (
        <div>
          <span>{input.status}</span>
          <button onClick={() => setTick((t) => t + 1)}>rerender</button>
        </div>
      );
    }
    render(<Harness />);
    expect(subscribeCallCount()).toBe(1);
    for (let i = 0; i < 10; i++) {
      act(() => { screen.getByRole('button').click(); });
    }
    // 每次渲染都重订阅会让计数随渲染次数增长并最终触发更新深度上限
    expect(subscribeCallCount()).toBe(1);
  });

  it('useNumericInputs keeps the aggregate subscription across re-renders', () => {
    useAppStore.setState(makeGraph('w-multi'));
    function Harness() {
      const [, setTick] = useState(0);
      const inputs = useNumericInputs('w-multi', ['value'] as const);
      return (
        <div>
          <span>{inputs.value.status}</span>
          <button onClick={() => setTick((t) => t + 1)}>rerender</button>
        </div>
      );
    }
    render(<Harness />);
    expect(subscribeCallCount()).toBe(1);
    for (let i = 0; i < 10; i++) {
      act(() => { screen.getByRole('button').click(); });
    }
    expect(subscribeCallCount()).toBe(1);
  });

  it('resubscribes only when the resolved port actually changes', () => {
    useAppStore.setState(makeGraph('w-switch'));
    function Harness() {
      const [, setTick] = useState(0);
      const input = useNumericInput('w-switch', 'value');
      return (
        <div>
          <span>{input.status}</span>
          <button onClick={() => setTick((t) => t + 1)}>rerender</button>
        </div>
      );
    }
    render(<Harness />);
    expect(subscribeCallCount()).toBe(1);
    // 端口解析结果变化 (连线切换到另一来源) 时才允许重建订阅
    act(() => {
      useAppStore.setState({
        rfEdges: [
          { id: 'e2', source: 'proto-1', sourceHandle: 'ch1', target: 'w-switch', targetHandle: 'value' },
        ],
      });
    });
    expect(subscribeCallCount()).toBe(2);
  });
});
