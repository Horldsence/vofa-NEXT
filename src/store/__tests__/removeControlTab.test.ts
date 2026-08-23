import { beforeEach, describe, expect, it, vi } from 'vitest';

// persist 中间件 localStorage 桩 (与其他 store 测试一致)
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

import { tauriMock } from '../../test/setup';
import { useAppStore } from '../appStore';
import type { Node } from '@xyflow/react';

const TRANSPORT_NODE: Node = {
  id: 'transport-1',
  type: 'transport',
  position: { x: 40, y: 40 },
  data: {
    global: true,
    config: { kind: 'TestData', params: { channels: 4, sample_rate: 100, signal: 'Sine' } },
    label: 'TestData',
  },
};

const PROTOCOL_NODE: Node = {
  id: 'protocol-1',
  type: 'protocol',
  position: { x: 300, y: 40 },
  data: {
    global: true,
    config: { kind: 'JustFloat', channels: 2 },
    convertTo: null,
    channels: 2,
    label: 'JustFloat',
  },
};

type InvokeCall = [string, unknown];

function invokeCalls(): InvokeCall[] {
  return tauriMock.invoke.mock.calls as unknown as InvokeCall[];
}

/// update_tab_graph(tabId) 与 remove_tab_graph 的调用下标
function syncAndRemoveIndices(syncTabId: string): { syncIdx: number; removeIdx: number } {
  const calls = invokeCalls();
  const syncIdx = calls.findIndex(
    ([cmd, args]) => cmd === 'update_tab_graph' && (args as { tabId: string }).tabId === syncTabId
  );
  const removeIdx = calls.findIndex(([cmd]) => cmd === 'remove_tab_graph');
  return { syncIdx, removeIdx };
}

describe('removeControlTab (后端全局节点归属)', () => {
  beforeEach(() => {
    tauriMock.invoke.mockClear();
    useAppStore.setState({
      controlTabs: [
        { id: 'default', name: 'Tab 1', widgets: [] },
        { id: 'tab2', name: 'Tab 2', widgets: [] },
      ],
      activeControlTabId: 'default',
      rfNodes: [TRANSPORT_NODE, PROTOCOL_NODE],
      rfEdges: [],
    } as never);
  });

  it('删除 tab: 先对存活 tab 调 update_tab_graph (重新托管全局节点), 再调 remove_tab_graph', async () => {
    useAppStore.getState().removeControlTab('tab2');

    await vi.waitFor(() => {
      expect(tauriMock.invoke).toHaveBeenCalledWith('remove_tab_graph', { tabId: 'tab2' });
    });

    const { syncIdx, removeIdx } = syncAndRemoveIndices('default');
    expect(syncIdx).toBeGreaterThanOrEqual(0);
    expect(removeIdx).toBeGreaterThan(syncIdx);

    // 存活 tab 的 sync 重新提交了全局节点 (tab_id 挂到存活 tab 名下)
    const syncArgs = invokeCalls()[syncIdx][1] as { nodes: { id: string; tab_id: string }[] };
    expect(syncArgs.nodes.find((n) => n.id === 'transport-1')?.tab_id).toBe('default');
    expect(syncArgs.nodes.find((n) => n.id === 'protocol-1')?.tab_id).toBe('default');
  });

  it('删除最后一个 tab: 重建 default tab, 同样先同步后移除', async () => {
    useAppStore.setState({
      controlTabs: [{ id: 'only', name: 'Tab 1', widgets: [] }],
      activeControlTabId: 'only',
    } as never);

    useAppStore.getState().removeControlTab('only');

    expect(useAppStore.getState().controlTabs.map((t) => t.id)).toEqual(['default']);
    await vi.waitFor(() => {
      expect(tauriMock.invoke).toHaveBeenCalledWith('remove_tab_graph', { tabId: 'only' });
    });
    const { syncIdx, removeIdx } = syncAndRemoveIndices('default');
    expect(syncIdx).toBeGreaterThanOrEqual(0);
    expect(removeIdx).toBeGreaterThan(syncIdx);
  });
});
