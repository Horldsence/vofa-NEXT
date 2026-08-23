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

/// custom schema 协议节点 (命名端口 speed/temp, 非 chN)
const CUSTOM_PROTOCOL_NODE: Node = {
  id: 'protocol-custom',
  type: 'protocol',
  position: { x: 300, y: 40 },
  data: {
    global: true,
    config: { kind: 'JustFloat', channels: 2 },
    convertTo: null,
    channels: 2,
    label: 'JustFloat',
    schema: {
      preset: 'custom',
      legacyConfig: null,
      decode: [
        { id: 'f0', type: 'field', fieldType: 'float32LE', portName: 'speed' },
        { id: 'f1', type: 'field', fieldType: 'float32LE', portName: 'temp' },
      ],
    },
  },
};
/// RawData 预设协议节点 (str 字符串口, 无 chN)
const RAWDATA_PROTOCOL_NODE: Node = {
  id: 'protocol-raw',
  type: 'protocol',
  position: { x: 300, y: 40 },
  data: {
    global: true,
    config: { kind: 'RawData' },
    convertTo: null,
    channels: 4,
    label: 'RawData',
    schema: { preset: 'rawData', legacyConfig: { kind: 'RawData' }, decode: [] },
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
    expect(ps!.kind.params).toMatchObject({ node_id: 'protocol-1', channels: 2, port_names: ['ch0', 'ch1'] });
    // 边原样提交 (source = 全局 Protocol 节点 id)
    expect(args.edges.some((e) => e.source === 'protocol-1' && e.source_handle === 'ch0')).toBe(true);
  });

  it('custom schema 命名端口边触发 ProtocolSource (port_names = 命名端口)', async () => {
    useAppStore.setState({
      rfNodes: [TRANSPORT_NODE, CUSTOM_PROTOCOL_NODE, GAUGE_NODE],
      rfEdges: [
        { id: 'e-speed', source: 'protocol-custom', sourceHandle: 'speed', target: 'w-gauge', targetHandle: 'value' },
      ] as Edge[],
    } as never);

    await syncTabGraphToBackend('default');

    const args = lastGraphArgs();
    const ps = args.nodes.find((n) => n.kind.kind === 'ProtocolSource');
    expect(ps).toBeDefined();
    expect(ps!.id).toBe('protocol-custom');
    expect(ps!.kind.params).toMatchObject({
      node_id: 'protocol-custom',
      channels: 2,
      port_names: ['speed', 'temp'],
    });
    // 命名端口边原样提交 (后端槽位支持命名)
    expect(args.edges.some((e) => e.source === 'protocol-custom' && e.source_handle === 'speed')).toBe(true);
    // Protocol 节点定义携带 schema
    const protocol = args.nodes.find((n) => n.id === 'protocol-custom' && n.kind.kind === 'Protocol');
    expect(protocol?.kind.params?.schema).toMatchObject({ preset: 'custom' });
  });

  it('RawData 协议节点 str 边触发 ProtocolSource (port_names = ["str"])', async () => {
    useAppStore.setState({
      rfNodes: [TRANSPORT_NODE, RAWDATA_PROTOCOL_NODE, GAUGE_NODE],
      rfEdges: [
        { id: 'e-str', source: 'protocol-raw', sourceHandle: 'str', target: 'w-gauge', targetHandle: 'value' },
      ] as Edge[],
    } as never);

    await syncTabGraphToBackend('default');

    const args = lastGraphArgs();
    const ps = args.nodes.find((n) => n.kind.kind === 'ProtocolSource');
    expect(ps).toBeDefined();
    expect(ps!.id).toBe('protocol-raw');
    expect(ps!.kind.params).toMatchObject({
      node_id: 'protocol-raw',
      channels: 1,
      port_names: ['str'],
    });
    // str 边原样提交 (后端 ProtocolSource str 端口写入字符串平面)
    expect(args.edges.some((e) => e.source === 'protocol-raw' && e.source_handle === 'str')).toBe(true);
  });

  it('RawData 协议节点不再产 chN 口 — chN 边不触发 ProtocolSource', async () => {
    useAppStore.setState({
      rfNodes: [TRANSPORT_NODE, RAWDATA_PROTOCOL_NODE, GAUGE_NODE],
      rfEdges: [
        { id: 'e-ch', source: 'protocol-raw', sourceHandle: 'ch0', target: 'w-gauge', targetHandle: 'value' },
      ] as Edge[],
    } as never);

    await syncTabGraphToBackend('default');

    const args = lastGraphArgs();
    expect(args.nodes.some((n) => n.kind.kind === 'ProtocolSource')).toBe(false);
  });

  it('custom schema 下 chN 等未知端口边不触发 ProtocolSource', async () => {
    useAppStore.setState({
      rfNodes: [TRANSPORT_NODE, CUSTOM_PROTOCOL_NODE, GAUGE_NODE],
      rfEdges: [
        { id: 'e-ch', source: 'protocol-custom', sourceHandle: 'ch0', target: 'w-gauge', targetHandle: 'value' },
      ] as Edge[],
    } as never);

    await syncTabGraphToBackend('default');

    const args = lastGraphArgs();
    expect(args.nodes.some((n) => n.kind.kind === 'ProtocolSource')).toBe(false);
  });

  it('无 chN 边时不产生 ProtocolSource; 其他 tab 的边不混入', async () => {    useAppStore.setState({
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

describe('seedInitialGraph (初始图: 设备→协议→RawData)', () => {
  beforeEach(() => {
    tauriMock.invoke.mockClear();
    useAppStore.setState({
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: ['w-raw'] }],
      activeControlTabId: 'default',
      rfNodes: [
        {
          id: 'w-raw', type: 'widget', position: { x: 560, y: 120 },
          data: { tabId: 'default', widget: { kind: 'RawData', params: { id: 'w-raw', label: 'RawData' } } },
        } as Node,
      ],
      rfEdges: [],
    } as never);
  });

  it('创建 TestData 设备 + JustFloat 协议节点与两条连线', async () => {
    useAppStore.getState().seedInitialGraph('w-raw');

    const { rfNodes, rfEdges } = useAppStore.getState();
    const transport = rfNodes.find((n) => n.type === 'transport');
    const protocol = rfNodes.find((n) => n.type === 'protocol');
    expect((transport?.data as { config: { kind: string } }).config.kind).toBe('TestData');
    expect((protocol?.data as { config: { kind: string } }).config.kind).toBe('JustFloat');

    // 设备.rx → 协议.in
    expect(rfEdges.some((e) => e.source === transport!.id && e.sourceHandle === 'rx'
      && e.target === protocol!.id && e.targetHandle === 'in')).toBe(true);
    // 协议.out → RawData (targetHandle 改写为动态端口 src:<id>:out)
    expect(rfEdges.some((e) => e.source === protocol!.id && e.sourceHandle === 'out'
      && e.target === 'w-raw' && e.targetHandle === `src:${protocol!.id}:out`)).toBe(true);

    // 图同步到后端
    await vi.waitFor(() => {
      expect(tauriMock.invoke).toHaveBeenCalledWith('update_tab_graph', expect.anything());
    });
  });
});


describe('图删除操作触发后端同步 (remove change 无 source/target)', () => {
  beforeEach(() => {
    tauriMock.invoke.mockClear();
    useAppStore.setState({
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: ['w-gauge'] }],
      activeControlTabId: 'default',
      rfNodes: [TRANSPORT_NODE, PROTOCOL_NODE, GAUGE_NODE],
      rfEdges: [
        { id: 'e-byte', source: 'transport-1', sourceHandle: 'rx', target: 'protocol-1', targetHandle: 'in' },
        { id: 'e-ch', source: 'protocol-1', sourceHandle: 'ch0', target: 'w-gauge', targetHandle: 'value' },
      ] as Edge[],
    } as never);
  });

  /// 等待 syncTabGraph 的 void Promise 落地
  const flushSync = () => vi.waitFor(() => {
    expect(tauriMock.invoke).toHaveBeenCalledWith('update_tab_graph', expect.anything());
  });

  it('删除边后同步后端, 后端边列表不再包含被删边', async () => {
    useAppStore.getState().onEdgesChange([{ id: 'e-ch', type: 'remove' }]);

    await flushSync();
    const args = lastGraphArgs();
    expect(args.edges.some((e) => e.id === 'e-ch')).toBe(false);
    // 未删除的字节边仍在
    expect(args.edges.some((e) => e.id === 'e-byte')).toBe(true);
  });

  it('删除全局节点间的字节边同样触发同步', async () => {
    useAppStore.getState().onEdgesChange([{ id: 'e-byte', type: 'remove' }]);

    await flushSync();
    const args = lastGraphArgs();
    expect(args.edges.some((e) => e.id === 'e-byte')).toBe(false);
    expect(args.edges.some((e) => e.id === 'e-ch')).toBe(true);
  });

  it('键盘删除 widget 节点后同步后端, 节点定义被移除', async () => {
    useAppStore.getState().onNodesChange([{ id: 'w-gauge', type: 'remove' }]);

    await flushSync();
    const args = lastGraphArgs();
    expect(args.nodes.some((n) => n.id === 'w-gauge')).toBe(false);
  });
});
