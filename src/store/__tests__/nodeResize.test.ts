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

const GAUGE_NODE: Node = {
  id: 'w-gauge',
  type: 'widget',
  position: { x: 560, y: 40 },
  width: 200,
  data: {
    tabId: 'default',
    widget: { kind: 'Gauge', params: { id: 'w-gauge', label: 'G', min: 0, max: 100, unit: '', channel: null } },
  },
};

function invokeCalls(): [string, Record<string, unknown>][] {
  return tauriMock.invoke.mock.calls as unknown as [string, Record<string, unknown>][];
}

function setNodePositionsCall(): Record<string, unknown> | undefined {
  const calls = invokeCalls().filter((c) => c[0] === 'set_node_positions');
  return calls[calls.length - 1]?.[1];
}

describe('控件节点尺寸 (NodeResizer 持久化)', () => {
  beforeEach(() => {
    tauriMock.invoke.mockClear();
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue(undefined);
    useAppStore.setState({
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: ['w-gauge'] }],
      activeControlTabId: 'default',
      widgets: [
        { kind: 'Gauge', params: { id: 'w-gauge', label: 'G', min: 0, max: 100, unit: '', channel: null } },
      ],
      rfNodes: [structuredClone(GAUGE_NODE)],
      rfEdges: [],
    } as never);
  });

  it('缩放批 (resizing=true + setAttributes) 写入显式尺寸但不落盘', () => {
    useAppStore.getState().onNodesChange([
      { id: 'w-gauge', type: 'dimensions', resizing: true, setAttributes: true, dimensions: { width: 320, height: 240 } },
    ]);
    const gauge = useAppStore.getState().rfNodes.find((n) => n.id === 'w-gauge');
    expect(gauge?.width).toBe(320);
    expect(gauge?.height).toBe(240);
    expect(setNodePositionsCall()).toBeUndefined();
  });

  it('缩放结束批 (resizing=false) 上报位置 + 尺寸到后端', () => {
    // 模拟真实拖拽: 先过程中的 setAttributes 批, 再 onEnd 的收尾批
    useAppStore.getState().onNodesChange([
      { id: 'w-gauge', type: 'dimensions', resizing: true, setAttributes: true, dimensions: { width: 320, height: 240 } },
    ]);
    useAppStore.getState().onNodesChange([
      { id: 'w-gauge', type: 'dimensions', resizing: false, dimensions: { width: 320, height: 240 } },
    ]);

    expect(setNodePositionsCall()).toEqual({
      positions: { 'w-gauge': { x: 560, y: 40, width: 320, height: 240 } },
    });
  });

  it('初始测量批 (无 resizing 标志) 不落盘', () => {
    useAppStore.getState().onNodesChange([
      { id: 'w-gauge', type: 'dimensions', dimensions: { width: 210, height: 96 } },
    ]);
    const gauge = useAppStore.getState().rfNodes.find((n) => n.id === 'w-gauge');
    // 只更新测量值, 显式尺寸不变
    expect(gauge?.width).toBe(200);
    expect(setNodePositionsCall()).toBeUndefined();
  });

  it('setWidgetNodeSize 更新显式尺寸并持久化; 空对象恢复自适应', () => {
    useAppStore.getState().setWidgetNodeSize('w-gauge', { width: 280, height: 200 });
    let gauge = useAppStore.getState().rfNodes.find((n) => n.id === 'w-gauge');
    expect(gauge?.width).toBe(280);
    expect(gauge?.height).toBe(200);
    expect(setNodePositionsCall()).toEqual({
      positions: { 'w-gauge': { x: 560, y: 40, width: 280, height: 200 } },
    });

    // 重置 — 尺寸字段移除, 上报载荷不带 width/height
    useAppStore.getState().setWidgetNodeSize('w-gauge', {});
    gauge = useAppStore.getState().rfNodes.find((n) => n.id === 'w-gauge');
    expect(gauge?.width).toBeUndefined();
    expect(gauge?.height).toBeUndefined();
    expect(setNodePositionsCall()).toEqual({
      positions: { 'w-gauge': { x: 560, y: 40 } },
    });
  });
});
