import { afterEach, describe, expect, it, vi } from 'vitest';
import { useState, type FunctionComponent } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { WidgetCard, type WidgetCardProps } from '../WidgetCard';

// 模块级稳定 children — memo 对 props 浅比较, children 必须跨渲染保持同一引用
const STABLE_CHILDREN = <span>stable content</span>;

/// 验证 app 已用 memo 包装的 WidgetCard。
/// React 19 的 memo 返回含 `.type` (真正渲染函数) 的对象 — 直接 spy `.type`
/// 即可精确统计 memo 组件内部的实际渲染次数。
function spyWidgetCardRender() {
  return vi.spyOn(WidgetCard as unknown as { type: FunctionComponent<WidgetCardProps> }, 'type');
}
describe('WidgetCard memoization', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('skips re-rendering when the parent re-renders with unchanged props', () => {
    const spy = spyWidgetCardRender();

    function App() {
      const [count, setCount] = useState(0);
      return (
        <div>
          <button type="button" onClick={() => setCount((c) => c + 1)}>
            bump {count}
          </button>
          <WidgetCard label="Stable" badge="A">
            {STABLE_CHILDREN}
          </WidgetCard>
        </div>
      );
    }

    render(<App />);
    expect(spy).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button'));
    fireEvent.click(screen.getByRole('button'));

    expect(screen.getByRole('button')).toHaveTextContent('bump 2');
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('still re-renders when a prop actually changes', () => {
    const spy = spyWidgetCardRender();

    function App({ label }: { label: string }) {
      return (
        <div>
          <WidgetCard label={label} badge="A">
            {STABLE_CHILDREN}
          </WidgetCard>
        </div>
      );
    }

    const { rerender } = render(<App label="Zero" />);
    expect(spy).toHaveBeenCalledTimes(1);

    rerender(<App label="One" />);

    expect(spy).toHaveBeenCalledTimes(2);
  });
});
