import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from '@testing-library/react';
import { AnimatedSwitch } from '../AnimatedSwitch';

function stubMatchMedia(matches = false) {
  vi.stubGlobal(
    'matchMedia',
    vi.fn().mockReturnValue({
      matches,
      media: '',
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })
  );
}

describe('AnimatedSwitch', () => {
  beforeEach(() => {
    stubMatchMedia();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('applies a pure CSS transform/opacity transition on switchKey change without remounting', () => {
    const { container, rerender } = render(
      <AnimatedSwitch switchKey="a">
        <div>content</div>
      </AnimatedSwitch>
    );
    const el = container.firstElementChild as HTMLElement;
    // 首次挂载不播放动画, 无内联 transition
    expect(el.style.transition).toBe('');

    rerender(
      <AnimatedSwitch switchKey="b">
        <div>content</div>
      </AnimatedSwitch>
    );

    // 同一 DOM 节点 (不 remount 子树), 切换仅通过 CSS transition 驱动
    expect(container.firstElementChild).toBe(el);
    expect(el.style.transition).toContain('opacity 180ms');
    expect(el.style.transition).toContain('transform 180ms');
    expect(el.style.opacity).toBe('1');
    expect(el.style.transform).toBe('none');
    expect(el.textContent).toBe('content');
  });

  it('resolves the slide direction from the ordered keys', () => {
    const { container, rerender } = render(
      <AnimatedSwitch switchKey="a" order={['a', 'b']} axis="x">
        <div>content</div>
      </AnimatedSwitch>
    );
    const el = container.firstElementChild as HTMLElement;

    rerender(
      <AnimatedSwitch switchKey="b" order={['a', 'b']} axis="x">
        <div>content</div>
      </AnimatedSwitch>
    );
    // a → b 向右移动: 起始态 translateX(16px) 过渡到 none; 终态为静止
    expect(el.style.transform).toBe('none');
    expect(el.style.transition).toContain('transform');

    rerender(
      <AnimatedSwitch switchKey="a" order={['a', 'b']} axis="x">
        <div>content</div>
      </AnimatedSwitch>
    );
    expect(el.style.transform).toBe('none');
  });

  it('skips animation when prefers-reduced-motion is enabled', () => {
    stubMatchMedia(true);
    const { container, rerender } = render(
      <AnimatedSwitch switchKey="a">
        <div>content</div>
      </AnimatedSwitch>
    );
    const el = container.firstElementChild as HTMLElement;

    rerender(
      <AnimatedSwitch switchKey="b">
        <div>content</div>
      </AnimatedSwitch>
    );
    expect(el.style.transition).toBe('');
  });
});
