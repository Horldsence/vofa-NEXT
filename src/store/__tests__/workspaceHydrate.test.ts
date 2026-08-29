import { beforeEach, describe, expect, it, vi } from 'vitest';

// persist 中间件 localStorage 桩 (与其他 store 测试一致)
vi.hoisted(() => {
  const store = new Map<string, string>();
  const localStorageMock = {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
    clear: () => void store.clear(),
    key: (index: number) => [...store.keys()][index] ?? null,
    get length() {
      return store.size;
    },
  };
  const g = globalThis as { localStorage?: unknown };
  g.localStorage = localStorageMock;
});

import { tauriMock } from '../../test/setup';
import { useAppStore } from '../appStore';
import {
  hydrateWorkspaceFromBackend,
  syncWorkspaceMeta,
} from '../appStoreHelpers';
import type { WorkspaceSnapshotPayload } from '../../lib/tauri/tauri';

const SNAPSHOT: WorkspaceSnapshotPayload = {
  version: 12,
  tabs: [
    { id: 'tab-a', name: '主控', widgets: ['w-gauge'] },
    { id: 'tab-b', name: '备援', widgets: [] },
  ],
  data_tabs: [
    { id: 'w-gauge', name: 'Gauge', type: 'pie', closable: true, widget_id: 'w-gauge' },
    { id: 'compile-errors-fixed', name: 'Compile Errors', type: 'compile-errors', closable: false },
  ],
  graphs: [
    {
      tab_id: 'tab-a',
      nodes: [
        {
          id: 'transport-1',
          tab_id: 'tab-a',
          kind: {
            kind: 'Transport',
            params: {
              config: {
                kind: 'TestData',
                params: { channels: 4, sample_rate: 100, signal: 'Sine' },
              },
            },
          },
        },
        { id: 'w-gauge', tab_id: 'tab-a', kind: { kind: 'Sink' } },
      ],
      edges: [
        {
          id: 'e1',
          source: 'transport-1',
          source_handle: 'rx',
          target: 'w-gauge',
          target_handle: 'value',
        },
      ],
      widgets: [
        {
          id: 'w-gauge',
          kind: 'Gauge',
          params: { id: 'w-gauge', label: 'G', min: 0, max: 100, unit: '', channel: null },
        },
      ],
    },
    {
      tab_id: 'tab-b',
      nodes: [],
      edges: [],
      widgets: [
        // 未知 kind — 水合时剔除, 不产生节点
        { id: 'w-ghost', kind: 'FutureWidget', params: { id: 'w-ghost' } },
      ],
    },
  ],
  positions: {
    'transport-1': { x: 33, y: 44 },
    'w-gauge': { x: 100, y: 120 },
  },
};

describe('hydrateWorkspaceFromBackend (启动水合)', () => {
  beforeEach(() => {
    tauriMock.invoke.mockClear();
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue(null);
    useAppStore.setState({
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: [] }],
      activeControlTabId: 'default',
      dataTabs: [
        { id: 'compile-errors-fixed', type: 'compile-errors', name: 'Compile Errors', closable: false },
        { id: 'compile-results-fixed', type: 'compile-results', name: 'Compile Results', closable: false },
      ],
      activeDataTabId: 'compile-results-fixed',
      widgets: [],
      rfNodes: [],
      rfEdges: [],
      graphVersion: null,
    } as never);
  });

  it('无持久化工作区 (workspace_get 返回 null) → 默认启动', async () => {
    expect(await hydrateWorkspaceFromBackend()).toBe(false);
    expect(useAppStore.getState().controlTabs[0].id).toBe('default');
  });

  it('水合快照覆盖本地: tabs / widgets / 画布 / 边 / 版本基线', async () => {
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue(SNAPSHOT);

    expect(await hydrateWorkspaceFromBackend()).toBe(true);

    const s = useAppStore.getState();
    expect(s.controlTabs.map((t) => t.id)).toEqual(['tab-a', 'tab-b']);
    expect(s.activeControlTabId).toBe('tab-a');
    expect(s.graphVersion).toBe(12);
    // widget 节点 (位置来自快照位置表) + 全局节点 (NodeDef 重建, 位置跟随)
    const gauge = s.rfNodes.find((n) => n.id === 'w-gauge');
    expect(gauge?.position).toEqual({ x: 100, y: 120 });
    expect((gauge?.data as { widget: { kind: string } }).widget.kind).toBe('Gauge');
    const transport = s.rfNodes.find((n) => n.id === 'transport-1');
    expect((transport?.data as { config: { kind: string } }).config.kind).toBe('TestData');
    expect(transport?.position).toEqual({ x: 33, y: 44 });
    expect(s.rfEdges).toEqual([
      { id: 'e1', source: 'transport-1', sourceHandle: 'rx', target: 'w-gauge', targetHandle: 'value' },
    ]);
    expect(s.widgets.map((w) => w.params.id)).toEqual(['w-gauge']);
  });

  it('数据面板水合 + fixed 两页兜底注入; 未知 kind 剔除', async () => {
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue(SNAPSHOT);
    await hydrateWorkspaceFromBackend();

    const s = useAppStore.getState();
    const ids = s.dataTabs.map((t) => t.id);
    expect(ids).toContain('w-gauge');
    expect(ids).toContain('compile-errors-fixed');
    // 快照缺 compile-results-fixed → 兜底补入
    expect(ids).toContain('compile-results-fixed');
    // 未知 kind 的 widget 不建节点
    expect(s.rfNodes.some((n) => n.id === 'w-ghost')).toBe(false);
    expect(s.widgets.some((w) => w.params.id === 'w-ghost')).toBe(false);
  });
});

describe('syncWorkspaceMeta (tab 元数据整表覆盖)', () => {
  beforeEach(() => {
    tauriMock.invoke.mockClear();
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue(undefined);
    useAppStore.setState({
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: ['w1'] }],
      dataTabs: [
        { id: 'compile-results-fixed', type: 'compile-results', name: 'Compile Results', closable: false },
        { id: 'can-1', type: 'can', name: 'CAN', closable: true },
      ],
    } as never);
  });

  it('控件 tab 与数据面板元数据按后端形态上报', async () => {
    syncWorkspaceMeta();

    await vi.waitFor(() => {
      expect(tauriMock.invoke).toHaveBeenCalledWith('workspace_set_tabs', expect.anything());
    });
    const call = (tauriMock.invoke.mock.calls as unknown as [string, Record<string, unknown>][]).find(
      (c) => c[0] === 'workspace_set_tabs'
    );
    expect(call![1].tabs).toEqual([{ id: 'default', name: 'Tab 1', widgets: ['w1'] }]);
    expect(call![1].dataTabs).toEqual([
      { id: 'compile-results-fixed', name: 'Compile Results', type: 'compile-results', closable: false },
      { id: 'can-1', name: 'CAN', type: 'can', closable: true, widget_id: undefined },
    ]);
  });
});
