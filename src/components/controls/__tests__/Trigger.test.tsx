import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen } from '@testing-library/react';

// 共享 mock 状态
const mockState = vi.hoisted(() => ({
  updateWidgetCalls: [] as Array<{ id: string; widget: unknown }>,
  graphInputValue: 0,
  // 后端图输出快照 (value/matched 走数值平面)
  graphOutputs: {} as Record<string, Record<string, number>>,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  Channel: vi.fn(),
}));

vi.mock('../../../store/appStore', () => ({
  useAppStore: (selector: (s: unknown) => unknown) => {
    const state = {
      updateWidget: (id: string, widget: unknown) => {
        mockState.updateWidgetCalls.push({ id, widget });
      },
      graphOutputs: mockState.graphOutputs,
      lang: 'zh' as const,
    };
    return selector(state);
  },
}));

vi.mock('../../../lib/hooks/useGraphInput', () => ({
  useGraphInput: () => mockState.graphInputValue,
}));

import { Trigger } from '../Trigger';
import type { WidgetConfig } from '../../../types';

const TRIGGER_ID = 'test-trigger-1';
const NOOP = () => {};

function makeWidget(overrides: Record<string, unknown> = {}): Extract<WidgetConfig, { kind: 'Trigger' }> {
  return {
    kind: 'Trigger',
    params: {
      id: TRIGGER_ID,
      label: 'TestTrigger',
      mode: 'manual',
      edge: 'level',
      defaultMiss: 0,
      command: 'HELLO',
      rules: [
        { id: 'r1', pattern: 'HELLO', matchType: 'exact', outputValue: 1, enabled: true },
      ],
      ...overrides,
    },
  } as Extract<WidgetConfig, { kind: 'Trigger' }>;
}

beforeEach(() => {
  mockState.updateWidgetCalls.length = 0;
  mockState.graphInputValue = 0;
  mockState.graphOutputs = {};
});

describe('Trigger widget', () => {
  it('renders default config with one rule and manual mode', () => {
    render(<Trigger widget={makeWidget()} onRemove={NOOP} />);
    expect(screen.getByText('TestTrigger')).toBeInTheDocument();
    expect(screen.getByText('手动')).toBeInTheDocument();
    expect(screen.getByText('自动')).toBeInTheDocument();
    // 规则行至少有一处显示 HELLO (摘要或展开后输入框)
    const helloNodes = screen.getAllByText('HELLO');
    expect(helloNodes.length).toBeGreaterThan(0);
  });

  it('switches to auto mode and persists via updateWidget', () => {
    render(<Trigger widget={makeWidget()} onRemove={NOOP} />);
    fireEvent.click(screen.getByText('自动'));
    expect(mockState.updateWidgetCalls.length).toBeGreaterThan(0);
    const calls = mockState.updateWidgetCalls.map((c) => c.widget) as Array<{ params: { mode: string } }>;
    expect(calls.some((c) => c.params.mode === 'auto')).toBe(true);
  });

  it('adds a new rule when + button clicked (regex type)', () => {
    render(<Trigger widget={makeWidget()} onRemove={NOOP} />);
    const addButtons = screen.getAllByRole('button', { name: /正则/ });
    fireEvent.click(addButtons[0]);
    expect(mockState.updateWidgetCalls.length).toBeGreaterThan(0);
    const calls = mockState.updateWidgetCalls.map((c) => c.widget) as Array<{ params: { rules: Array<{ matchType: string }> } }>;
    expect(calls.some((c) => c.params.rules.some((r) => r.matchType === 'regex'))).toBe(true);
  });

  it('removes a rule when delete button clicked', () => {
    render(<Trigger widget={makeWidget({
      rules: [
        { id: 'r1', pattern: 'A', matchType: 'exact', outputValue: 1, enabled: true },
        { id: 'r2', pattern: 'B', matchType: 'exact', outputValue: 2, enabled: true },
      ],
    })} onRemove={NOOP} />);
    const removeButtons = screen.getAllByTitle('删除');
    expect(removeButtons.length).toBe(2);
    fireEvent.click(removeButtons[0]);
    const calls = mockState.updateWidgetCalls.map((c) => c.widget) as Array<{ params: { rules: Array<{ id: string }> } }>;
    // 删除后某次 updateWidget 调用里 rules 数组只剩 1 个
    expect(calls.some((c) => c.params.rules.length === 1 && c.params.rules[0]?.id === 'r2')).toBe(true);
  });

  it('manual 模式: 编辑 command 经 updateWidget 同步 (后端每帧以当前 command 求值)', () => {
    render(<Trigger widget={makeWidget({ command: 'HELLO' })} onRemove={NOOP} />);
    const textarea = screen.getByPlaceholderText(/GET_TEMP/i);
    fireEvent.change(textarea, { target: { value: 'PING' } });
    const calls = mockState.updateWidgetCalls.map((c) => c.widget) as Array<{ params: { command: string } }>;
    expect(calls.some((c) => c.params.command === 'PING')).toBe(true);
  });

  it('不再渲染 Fire 按钮 (求值由后端驱动, 前端无触发入口)', () => {
    render(<Trigger widget={makeWidget()} onRemove={NOOP} />);
    expect(screen.queryByRole('button', { name: /Fire/ })).toBeNull();
  });

  it('结果区读后端图输出快照 (graphOutputs[自己id].value/matched)', () => {
    mockState.graphOutputs = { [TRIGGER_ID]: { value: 8, matched: 1 } };
    render(<Trigger widget={makeWidget()} onRemove={NOOP} />);
    expect(screen.getByText('✓ YES')).toBeInTheDocument();
    expect(screen.getByText('8.0000')).toBeInTheDocument();
  });

  it('后端尚未产出 (graphOutputs 无该节点) 时不显示结果区', () => {
    render(<Trigger widget={makeWidget()} onRemove={NOOP} />);
    expect(screen.queryByText('✓ YES')).toBeNull();
    expect(screen.queryByText('✗ NO')).toBeNull();
  });

  it('renders AutoPanel when mode is auto', () => {
    render(<Trigger widget={makeWidget({ mode: 'auto' })} onRemove={NOOP} />);
    expect(screen.getByText(/上游 trigger/)).toBeInTheDocument();
    expect(screen.getByText(/电平/)).toBeInTheDocument();
    expect(screen.getByText(/上升沿/)).toBeInTheDocument();
  });

  it('auto 模式: 面板展示上游 trigger 端口实时值 (匹配本身在后端)', () => {
    mockState.graphInputValue = 143.7361;
    render(<Trigger widget={makeWidget({ mode: 'auto', edge: 'level' })} onRemove={NOOP} />);
    expect(screen.getAllByText(/143\.7361/).length).toBeGreaterThan(0);
  });
});
