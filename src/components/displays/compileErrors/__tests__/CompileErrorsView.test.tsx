import { describe, expect, it, vi, beforeEach } from 'vitest';

// 该 vitest jsdom 环境未启用 localStorage — dockStore/layoutStore 的 persist
// 中间件在 setState 时会写入 storage, 需在导入 store 前提供内存桩
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

import { fireEvent, render, screen } from '@testing-library/react';
import { useAppStore } from '../../../../store/appStore';
import { useDockStore } from '../../../../store/dockStore';
import { CompileErrorsView } from '../CompileErrorsView';
import type { CompileReport } from '../../../../store/slices/compileError';

describe('CompileErrorsView', () => {
  beforeEach(() => {
    // Reset stores
    useAppStore.setState({
      lang: 'en',
      errorTabs: [],
      tabErrors: {},
      tabStates: {},
      controlTabs: [
        { id: 'tab-1', name: 'Control Tab 1', widgets: [] },
        { id: 'tab-2', name: 'Control Tab 2', widgets: [] },
      ],
    });

    useDockStore.setState({
      cards: {
        'card-1': {
          id: 'card-1',
          kind: 'control',
          tabIds: ['tab-1', 'tab-2'],
          activeTabId: 'tab-1',
        },
      },
    });
  });

  it('renders empty state when there are no compile errors', () => {
    render(<CompileErrorsView />);
    expect(screen.getByText('No compile errors')).toBeInTheDocument();
  });

  it('renders compile errors grouped by tab', () => {
    const report1: CompileReport = {
      error: { kind: 'value_cycle', cycle: ['nodeA', 'nodeB', 'nodeA'] },
      nodes: ['nodeA'],
      edges: [],
    };

    useAppStore.setState({
      errorTabs: ['tab-1'],
      tabErrors: { 'tab-1': report1 },
      tabStates: { 'tab-1': 'error' },
    });

    render(<CompileErrorsView />);

    // Header info
    expect(screen.getByText('Compile Errors')).toBeInTheDocument();
    expect(screen.getByText('1 tab(s)')).toBeInTheDocument();

    // Group info
    expect(screen.getByText('Control Tab 1')).toBeInTheDocument();
    expect(screen.getByText('(1 node(s))')).toBeInTheDocument();

    // Error details inside the group
    expect(screen.getByText('nodeA')).toBeInTheDocument();
    expect(screen.getByText('Value cycle: nodeA → nodeB → nodeA')).toBeInTheDocument();
  });

  it('filters out resolved compile errors and shows empty state when compile state recovers to ok', () => {
    const report1: CompileReport = {
      error: { kind: 'value_cycle', cycle: ['nodeA', 'nodeB', 'nodeA'] },
      nodes: ['nodeA'],
      edges: [],
    };

    // Cumulative state: still in errorTabs, but tabState has recovered to ok
    useAppStore.setState({
      errorTabs: ['tab-1'],
      tabErrors: { 'tab-1': report1 },
      tabStates: { 'tab-1': 'ok' },
    });

    render(<CompileErrorsView />);

    // Since the error is repaired, activeErrors should be empty, showing "No compile errors" empty state
    expect(screen.getByText('No compile errors')).toBeInTheDocument();
    expect(screen.queryByText('Control Tab 1')).not.toBeInTheDocument();
    expect(screen.getByText('0 tab(s)')).toBeInTheDocument();
  });

  it('triggers fly-to flow when clicking map pin', () => {
    const report1: CompileReport = {
      error: { kind: 'node_not_found', id: 'nodeA' },
      nodes: ['nodeA'],
      edges: [],
    };

    const requestFlyToSpy = vi.fn();
    useAppStore.setState({
      errorTabs: ['tab-1'],
      tabErrors: { 'tab-1': report1 },
      tabStates: { 'tab-1': 'error' },
      requestFlyTo: requestFlyToSpy,
    });

    const setActiveTabSpy = vi.fn();
    const setFocusedCardSpy = vi.fn();
    useDockStore.setState({
      cards: {
        'card-1': {
          id: 'card-1',
          kind: 'control',
          tabIds: ['tab-1'],
          activeTabId: 'tab-1',
        },
      },
      setActiveTab: setActiveTabSpy,
      setFocusedCard: setFocusedCardSpy,
    });

    render(<CompileErrorsView />);

    const mapPinBtn = screen.getByRole('button', { name: 'Fly to node' });
    expect(mapPinBtn).toBeInTheDocument();

    fireEvent.click(mapPinBtn);

    // Should switch to control card & focus
    expect(setActiveTabSpy).toHaveBeenCalledWith('card-1', 'tab-1');
    expect(setFocusedCardSpy).toHaveBeenCalledWith('card-1');

    // Should request fly-to
    expect(requestFlyToSpy).toHaveBeenCalledWith('nodeA', 'tab-1');
  });

  it('allows collapsing and expanding groups', () => {
    const report1: CompileReport = {
      error: { kind: 'byte_cycle', cycle: ['nodeC', 'nodeD', 'nodeC'] },
      nodes: ['nodeC'],
      edges: [],
    };

    useAppStore.setState({
      errorTabs: ['tab-1'],
      tabErrors: { 'tab-1': report1 },
      tabStates: { 'tab-1': 'error' },
    });

    render(<CompileErrorsView />);

    // Initially open, details visible
    expect(screen.getByText('nodeC')).toBeInTheDocument();

    // Click header to collapse
    const groupHeader = screen.getByText('Control Tab 1');
    fireEvent.click(groupHeader);

    // Details should be hidden
    expect(screen.queryByText('nodeC')).not.toBeInTheDocument();

    // Click again to expand
    fireEvent.click(groupHeader);
    expect(screen.getByText('nodeC')).toBeInTheDocument();
  });
});
