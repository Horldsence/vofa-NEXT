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
import { syncTabGraphToBackend } from '../appStoreHelpers';
import type { Node, Edge } from '@xyflow/react';

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

const GAUGE_NODE: Node = {
  id: 'w-gauge',
  type: 'widget',
  position: { x: 560, y: 40 },
  data: {
    tabId: 'default',
    widget: { kind: 'Gauge', params: { id: 'w-gauge', label: 'G', min: 0, max: 100, unit: '', channel: null } },
  },
};


/// 取最近一次 update_tab_graph 调用参数 (invoke mock 类型为无参元组, 统一在此断言)
function lastGraphArgs(): {
  nodes: { id: string; tab_id: string; kind: { kind: string; params?: Record<string, unknown> } }[];
  edges: { id: string; source: string; source_handle: string; target: string; target_handle: string }[];
} {
  const calls = tauriMock.invoke.mock.calls as unknown as [string, unknown][];
  const call = calls.find((c) => c[0] === 'update_tab_graph');
  if (!call) throw new Error('update_tab_graph 未被调用');
  return call[1] as ReturnType<typeof lastGraphArgs>;
}

describe('syncTabGraphToBackend (图节点 + 字节边)', () => {
  beforeEach(() => {
    tauriMock.invoke.mockClear();
    useAppStore.setState({
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: ['w-gauge'] }],
      activeControlTabId: 'default',
      rfNodes: [TRANSPORT_NODE, PROTOCOL_NODE, GAUGE_NODE],
      rfEdges: [],
    } as never);
  });

  it('提交包含全局 Transport/Protocol 节点定义与字节边', async () => {
    useAppStore.setState({
      rfEdges: [
        { id: 'e-byte', source: 'transport-1', sourceHandle: 'rx', target: 'protocol-1', targetHandle: 'in' },
      ] as Edge[],
    } as never);

    await syncTabGraphToBackend('default');

    expect(tauriMock.invoke).toHaveBeenCalledWith('update_tab_graph', expect.objectContaining({
      tabId: 'default',
    }));
    const args = lastGraphArgs();
    // 全局节点定义 (snake_case 边)
    const transport = args.nodes.find((n) => n.id === 'transport-1');
    expect(transport?.kind.kind).toBe('Transport');
    const protocol = args.nodes.find((n) => n.id === 'protocol-1' && n.kind.kind === 'Protocol');
    expect(protocol?.kind.params).toMatchObject({ config: { kind: 'JustFloat', channels: 2 }, convert_to: null });
    // 字节边原样提交
    expect(args.edges).toContainEqual({
      id: 'e-byte', source: 'transport-1', source_handle: 'rx', target: 'protocol-1', target_handle: 'in',
    });
    // widget 节点
    expect(args.nodes.some((n) => n.id === 'w-gauge' && n.kind.kind === 'Sink')).toBe(true);
  });

  it('chN 数值边触发 ProtocolSource 定义 (id = 全局 Protocol 节点 id)', async () => {
    useAppStore.setState({
      rfEdges: [
        { id: 'e-ch', source: 'protocol-1', sourceHandle: 'ch0', target: 'w-gauge', targetHandle: 'value' },
      ] as Edge[],
    } as never);

    await syncTabGraphToBackend('default');

    const args = lastGraphArgs();
    const ps = args.nodes.find((n) => n.kind.kind === 'ProtocolSource');
    expect(ps).toBeDefined();
    expect(ps!.id).toBe('protocol-1');
    expect(ps!.kind.params).toMatchObject({ node_id: 'protocol-1', channels: 2 });
    // 边原样提交 (source = 全局 Protocol 节点 id)
    expect(args.edges.some((e) => e.source === 'protocol-1' && e.source_handle === 'ch0')).toBe(true);
  });

  it('无 chN 边时不产生 ProtocolSource; 其他 tab 的边不混入', async () => {
    useAppStore.setState({
      controlTabs: [
        { id: 'default', name: 'Tab 1', widgets: ['w-gauge'] },
        { id: 'tab2', name: 'Tab 2', widgets: [] },
      ],
      rfEdges: [
        // tab2 的数值边 (目标不在 default tab)
        { id: 'e-other', source: 'protocol-1', sourceHandle: 'ch1', target: 'w-other', targetHandle: 'value' },
      ] as Edge[],
      rfNodes: [
        TRANSPORT_NODE,
        PROTOCOL_NODE,
        GAUGE_NODE,
        {
          id: 'w-other', type: 'widget', position: { x: 0, y: 0 },
          data: { tabId: 'tab2', widget: { kind: 'Gauge', params: { id: 'w-other', label: 'G2', min: 0, max: 100, unit: '', channel: null } } },
        } as Node,
      ],
    } as never);

    await syncTabGraphToBackend('default');

    const args = lastGraphArgs();
    expect(args.nodes.some((n) => n.kind.kind === 'ProtocolSource')).toBe(false);
    expect(args.edges.some((e) => e.id === 'e-other')).toBe(false);
    expect(args.nodes.some((n) => n.id === 'w-other')).toBe(false);
  });
});
