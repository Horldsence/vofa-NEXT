import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, fireEvent, screen, waitFor } from '@testing-library/react';

// 共享 mock 状态
const mockState = vi.hoisted(() => ({
  matchTriggerCommandCalls: [] as Array<{ defaultMiss: number; cmd: string; numeric: number | null }>,
  submitCustomOutputCalls: [] as Array<{ id: string; outputs: unknown }>,
  updateWidgetCalls: [] as Array<{ id: string; widget: unknown }>,
  graphInputValue: 0,
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
      submitCustomOutput: (id: string, outputs: unknown) => {
        mockState.submitCustomOutputCalls.push({ id, outputs });
      },
      submitCustomTextOutput: vi.fn(),
      setInputValue: vi.fn(),
      lang: 'zh' as const,
    };
    return selector(state);
  },
}));

vi.mock('../../../lib/hooks/useGraphInput', () => ({
  useGraphInput: () => mockState.graphInputValue,
}));

vi.mock('../../../lib/tauri/tauri', () => ({
  api: {
    matchTriggerCommand: vi.fn(async (_rules: unknown, defaultMiss: number, _defaultMissText: string, command: string, numeric: number | null) => {
      mockState.matchTriggerCommandCalls.push({ defaultMiss, cmd: command, numeric });
      return { value: defaultMiss + 1, matched: true, text: '', outputType: 'number' };
    }),
  },
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
  mockState.matchTriggerCommandCalls.length = 0;
  mockState.submitCustomOutputCalls.length = 0;
  mockState.updateWidgetCalls.length = 0;
  mockState.graphInputValue = 0;
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

  it('Fire button calls matchTriggerCommand and submitCustomOutput', async () => {
    render(<Trigger widget={makeWidget({ defaultMiss: 7, command: 'TEST' })} onRemove={NOOP} />);
    // 右侧 Fire 按钮是唯一的 <button> + Zap 图标 + 含 "Fire" 文本
    const fireBtn = screen.getByRole('button', { name: /Fire/ });
    fireEvent.click(fireBtn);
    await waitFor(() => {
      expect(mockState.matchTriggerCommandCalls).toContainEqual({ defaultMiss: 7, cmd: 'TEST', numeric: null });
    });
    await waitFor(() => {
      expect(mockState.submitCustomOutputCalls).toContainEqual(
        expect.objectContaining({ id: TRIGGER_ID, outputs: { value: 8, matched: 1 } }),
      );
    });
  });

  it('Fire 时把数字命令解析为 numeric 传给后端 (range 规则依赖)', async () => {
    render(<Trigger widget={makeWidget({ command: '22' })} onRemove={NOOP} />);
    fireEvent.click(screen.getByRole('button', { name: /Fire/ }));
    await waitFor(() => {
      expect(mockState.matchTriggerCommandCalls).toContainEqual({ defaultMiss: 0, cmd: '22', numeric: 22 });
    });
  });

  it('renders AutoPanel when mode is auto', () => {
    render(<Trigger widget={makeWidget({ mode: 'auto' })} onRemove={NOOP} />);
    expect(screen.getByText(/上游 trigger/)).toBeInTheDocument();
    expect(screen.getByText(/电平/)).toBeInTheDocument();
    expect(screen.getByText(/上升沿/)).toBeInTheDocument();
  });

  it('auto 模式: 电平有效时以上游值作为命令与数值匹配, 而非手动命令文本', async () => {
    mockState.graphInputValue = 143.7361;
    render(<Trigger widget={makeWidget({ mode: 'auto', edge: 'level', command: '100.5552' })} onRemove={NOOP} />);
    await waitFor(() => {
      expect(mockState.matchTriggerCommandCalls).toContainEqual({ defaultMiss: 0, cmd: '143.7361', numeric: 143.7361 });
    });
  });
});