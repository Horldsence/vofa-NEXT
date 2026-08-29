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
    // 模拟后端响应: 派生端口表按节点返回
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue({ nodes: [] });
    useAppStore.setState({
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: ['w-gauge'] }],
      activeControlTabId: 'default',
      rfNodes: [TRANSPORT_NODE, PROTOCOL_NODE, GAUGE_NODE],
      rfEdges: [],
      derivedPorts: {},
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

  it('前端不再下发 ProtocolSource NodeDef (后端 cmd_graph::inject_protocol_sources 接管)', async () => {
    useAppStore.setState({
      rfEdges: [
        { id: 'e-ch', source: 'protocol-1', sourceHandle: 'ch0', target: 'w-gauge', targetHandle: 'value' },
      ] as Edge[],
    } as never);

    await syncTabGraphToBackend('default');

    const args = lastGraphArgs();
    // 关键契约: 前端下发的 nodes 不含 ProtocolSource — 后端按边自动注入
    expect(args.nodes.some((n) => n.kind.kind === 'ProtocolSource')).toBe(false);
    // 全局 Protocol 定义仍存在 (id = protocol-1)
    expect(args.nodes.some((n) => n.id === 'protocol-1' && n.kind.kind === 'Protocol')).toBe(true);
    // 数值边原样提交 (后端据其派生 ProtocolSource)
    expect(args.edges.some((e) => e.source === 'protocol-1' && e.source_handle === 'ch0')).toBe(true);
  });

  it('custom schema 节点仍透传 schema; preset 节点 schema 强制省略 (后端 schema 工厂下沉)', async () => {
    useAppStore.setState({
      rfNodes: [TRANSPORT_NODE, CUSTOM_PROTOCOL_NODE, GAUGE_NODE],
      rfEdges: [
        { id: 'e-speed', source: 'protocol-custom', sourceHandle: 'speed', target: 'w-gauge', targetHandle: 'value' },
      ] as Edge[],
    } as never);

    await syncTabGraphToBackend('default');

    const args = lastGraphArgs();
    // custom schema 节点: schema 仍携带 (后端走 Custom 路径)
    const customProto = args.nodes.find((n) => n.id === 'protocol-custom' && n.kind.kind === 'Protocol');
    expect(customProto?.kind.params?.schema).toMatchObject({ preset: 'custom' });
  });

  it('preset (JustFloat/FireWater/RawData/Slcan/CandleLight/LogicDecode) schema 一律省略', async () => {
    useAppStore.setState({
      rfNodes: [TRANSPORT_NODE, PROTOCOL_NODE, RAWDATA_PROTOCOL_NODE, GAUGE_NODE],
      rfEdges: [
        { id: 'e-byte', source: 'transport-1', sourceHandle: 'rx', target: 'protocol-1', targetHandle: 'in' },
      ] as Edge[],
    } as never);

    await syncTabGraphToBackend('default');

    const args = lastGraphArgs();
    // 两个 preset 节点的 schema 字段均为 null (serde 省略)
    const justFloat = args.nodes.find((n) => n.id === 'protocol-1' && n.kind.kind === 'Protocol');
    expect(justFloat?.kind.params?.schema).toBeUndefined();
    const rawData = args.nodes.find((n) => n.id === 'protocol-raw' && n.kind.kind === 'Protocol');
    expect(rawData?.kind.params?.schema).toBeUndefined();
  });

  it('后端响应 GraphDerived 自动写入 derivedPorts store', async () => {
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue({
      nodes: [
        { node_id: 'protocol-1', ports: [{ name: 'ch0', domain: 'F32' }, { name: 'ch1', domain: 'F32' }], effective_channels: 2 },
      ],
    });
    useAppStore.setState({
      rfEdges: [
        { id: 'e-ch', source: 'protocol-1', sourceHandle: 'ch0', target: 'w-gauge', targetHandle: 'value' },
      ] as Edge[],
    } as never);

    await syncTabGraphToBackend('default');

    const derived = useAppStore.getState().derivedPorts;
    expect(derived['protocol-1']?.ports.map((p) => p.name)).toEqual(['ch0', 'ch1']);
    expect(derived['protocol-1']?.effective_channels).toBe(2);
  });

  it('提交携带 widget 配置记录与画布位置 (配置模型后端权威)', async () => {
    await syncTabGraphToBackend('default');

    const call = (tauriMock.invoke.mock.calls as unknown as [string, Record<string, unknown>][])
      .find((c) => c[0] === 'update_tab_graph');
    expect(call).toBeDefined();
    const args = call![1];
    // widget 记录 = {id, kind, params} 透传
    const widgets = args.widgets as Array<{ id: string; kind: string; params: Record<string, unknown> }>;
    expect(widgets).toEqual([
      expect.objectContaining({ id: 'w-gauge', kind: 'Gauge' }),
    ]);
    expect(widgets[0].params).toMatchObject({ min: 0, max: 100 });
    // 位置表覆盖本 tab 可见节点 (widget + 全局), 不含其他 tab 节点
    const positions = args.positions as Record<string, { x: number; y: number }>;
    expect(positions['w-gauge']).toEqual({ x: 560, y: 40 });
    expect(positions['transport-1']).toEqual({ x: 40, y: 40 });
    expect(positions['w-other']).toBeUndefined();
  });

  it('其他 tab 的边不混入', async () => {
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
    expect(args.edges.some((e) => e.id === 'e-other')).toBe(false);
    expect(args.nodes.some((n) => n.id === 'w-other')).toBe(false);
  });
});

describe('seedInitialGraph (初始图: 设备→协议→RawData)', () => {
  beforeEach(() => {
    tauriMock.invoke.mockClear();
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue({ nodes: [] });
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
      derivedPorts: {},
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
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue({ nodes: [] });
    useAppStore.setState({
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: ['w-gauge'] }],
      activeControlTabId: 'default',
      rfNodes: [TRANSPORT_NODE, PROTOCOL_NODE, GAUGE_NODE],
      rfEdges: [
        { id: 'e-byte', source: 'transport-1', sourceHandle: 'rx', target: 'protocol-1', targetHandle: 'in' },
        { id: 'e-ch', source: 'protocol-1', sourceHandle: 'ch0', target: 'w-gauge', targetHandle: 'value' },
      ] as Edge[],
      derivedPorts: {},
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