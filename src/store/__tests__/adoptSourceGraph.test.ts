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
import { adoptSourceGraph, syncTabGraphToBackend } from '../appStoreHelpers';
import type { GraphSourceEventPayload } from '../../lib/tauri/tauri';
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

const BYTE_EDGE: Edge = {
  id: 'e-byte',
  source: 'transport-1',
  sourceHandle: 'rx',
  target: 'protocol-1',
  targetHandle: 'in',
};

function seedStore() {
  useAppStore.setState({
    controlTabs: [{ id: 'default', name: 'Tab 1', widgets: ['w-gauge'] }],
    activeControlTabId: 'default',
    rfNodes: [TRANSPORT_NODE, PROTOCOL_NODE, GAUGE_NODE],
    rfEdges: [BYTE_EDGE],
    derivedPorts: {},
    graphVersion: 1,
  } as never);
}

const SOURCE_EVENT: GraphSourceEventPayload = {
  tab_id: 'default',
  version: 7,
  nodes: [],
  edges: [
    { id: 'e-byte', source: 'transport-1', source_handle: 'rx', target: 'protocol-1', target_handle: 'in' },
    {
      id: 'e-val',
      source: 'protocol-1',
      source_handle: 'ch0',
      target: 'w-gauge',
      target_handle: 'value',
    },
  ],
};

describe('adoptSourceGraph (画布 = 权威源图的投影)', () => {
  beforeEach(() => {
    tauriMock.invoke.mockClear();
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue({ nodes: [] });
    seedStore();
  });

  it('该 tab 的边被替换为权威集, 版本号写回', () => {
    adoptSourceGraph(SOURCE_EVENT);

    const { rfEdges, graphVersion } = useAppStore.getState();
    expect(graphVersion).toBe(7);
    expect(rfEdges.map((e) => e.id).sort()).toEqual(['e-byte', 'e-val']);
    // snake_case → camelCase
    const val = rfEdges.find((e) => e.id === 'e-val');
    expect(val?.sourceHandle).toBe('ch0');
    expect(val?.targetHandle).toBe('value');
  });

  it('后端编译认可的连线逐字采纳, 不做前端有效性判断 (含瞬时不渲染的端口)', () => {
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue({
      nodes: [],
      version: 7,
    });
    const danglingEdge = {
      id: 'e-dangling',
      source: 'protocol-1',
      source_handle: 'ch1',
      target: 'w-gauge',
      target_handle: 'nope',
    };
    adoptSourceGraph({
      ...SOURCE_EVENT,
      edges: [...SOURCE_EVENT.edges, danglingEdge],
    });

    // 前端不判断端口存在性 — 边保留, 是否有效由后端编译权威裁决
    expect(
      useAppStore.getState().rfEdges.some((e) => e.id === 'e-dangling')
    ).toBe(true);
  });

  it('缺失的全局节点 (NodeDef) 自动补建', () => {
    adoptSourceGraph({
      ...SOURCE_EVENT,
      nodes: [
        {
          id: 'transport-9',
          tab_id: 'default',
          kind: {
            kind: 'Transport',
            params: {
              config: {
                kind: 'Udp',
                params: { local_addr: '0.0.0.0', remote_addr: '127.0.0.1', local_port: 1234, remote_port: 9999 },
              },
            },
          },
        },
      ],
      edges: [
        ...SOURCE_EVENT.edges,
        { id: 'e-new', source: 'transport-9', source_handle: 'rx', target: 'protocol-1', target_handle: 'in' },
      ],
    });

    const { rfNodes } = useAppStore.getState();
    const added = rfNodes.find((n) => n.id === 'transport-9');
    expect(added?.type).toBe('transport');
    expect((added?.data as { config: { kind: string } }).config.kind).toBe('Udp');
    expect(useAppStore.getState().rfEdges.some((e) => e.id === 'e-new')).toBe(true);
  });

  it('无变化时不触发 store 更新', () => {
    const before = useAppStore.getState().rfEdges;
    adoptSourceGraph({
      tab_id: 'default',
      version: 1,
      nodes: [],
      edges: [
        { id: 'e-byte', source: 'transport-1', source_handle: 'rx', target: 'protocol-1', target_handle: 'in' },
      ],
    });
    expect(useAppStore.getState().rfEdges).toBe(before);
  });
});

describe('adoptSourceGraph (widget 配置记录收敛 — 配置模型后端权威)', () => {
  beforeEach(() => {
    tauriMock.invoke.mockClear();
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue({ nodes: [] });
    seedStore();
  });

  it('事件携带记录时: 缺失的 widget 节点按记录补建 (外部纯 widget 图完整渲染)', () => {
    adoptSourceGraph({
      ...SOURCE_EVENT,
      widgets: [
        {
          id: 'w-remote',
          kind: 'Knob',
          params: { id: 'w-remote', label: 'K', min: 0, max: 10, step: 1, value: 5, unit: '', channel: null },
        },
      ],
      positions: { 'w-remote': { x: 11, y: 22 } },
    });

    const { rfNodes, widgets } = useAppStore.getState();
    const added = rfNodes.find((n) => n.id === 'w-remote');
    expect(added?.type).toBe('widget');
    expect(added?.position).toEqual({ x: 11, y: 22 });
    expect((added?.data as { widget: { kind: string } }).widget.kind).toBe('Knob');
    expect((added?.data as { tabId: string }).tabId).toBe('default');
    expect(widgets.some((w) => w.params.id === 'w-remote')).toBe(true);
  });

  it('已有 widget 参数变化时更新节点与 widgets 数组; 参数未变时不触碰', () => {
    const changed = useAppStore.getState();
    adoptSourceGraph({
      ...SOURCE_EVENT,
      widgets: [
        {
          id: 'w-gauge',
          kind: 'Gauge',
          params: { id: 'w-gauge', label: 'G2', min: 0, max: 200, unit: 'V', channel: null },
        },
      ],
    });
    const after = useAppStore.getState();
    const node = after.rfNodes.find((n) => n.id === 'w-gauge');
    expect((node?.data as { widget: { params: { max: number } } }).widget.params.max).toBe(200);
    const flat = after.widgets.find((w) => w.params.id === 'w-gauge');
    expect((flat?.params as unknown as { max: number }).max).toBe(200);
    void changed;
  });

  it('记录集中缺少的本地 widget 节点被删除 (外部删除生效), 其边一并剔除', () => {
    adoptSourceGraph({
      ...SOURCE_EVENT,
      edges: [SOURCE_EVENT.edges[0]], // 只保留字节边, e-val (→ w-gauge) 已被外部删除
      widgets: [], // 空 = 该 tab 无 widget
    });

    const { rfNodes, rfEdges, widgets, controlTabs } = useAppStore.getState();
    expect(rfNodes.some((n) => n.id === 'w-gauge')).toBe(false);
    expect(rfEdges.some((e) => e.id === 'e-val')).toBe(false);
    expect(widgets.some((w) => w.params.id === 'w-gauge')).toBe(false);
    expect(controlTabs[0].widgets).toEqual([]);
  });

  it('事件未携带 widgets 字段 (旧契约) 时画布 widget 保持不动', () => {
    adoptSourceGraph(SOURCE_EVENT);
    expect(useAppStore.getState().rfNodes.some((n) => n.id === 'w-gauge')).toBe(true);
    expect(useAppStore.getState().controlTabs[0].widgets).toEqual(['w-gauge']);
  });

  it('未知 widget kind 落为占位控件 (不丢弃), 引用它的边保留', () => {
    adoptSourceGraph({
      ...SOURCE_EVENT,
      widgets: [
        { id: 'w-ghost', kind: 'FutureWidget', params: { id: 'w-ghost' } },
        {
          id: 'w-gauge',
          kind: 'Gauge',
          params: { id: 'w-gauge', label: 'G', min: 0, max: 100, unit: '', channel: null },
        },
      ],
      edges: [
        ...SOURCE_EVENT.edges,
        { id: 'e-ghost', source: 'protocol-1', source_handle: 'ch1', target: 'w-ghost', target_handle: 'in' },
      ],
    });

    // 占位节点存在 (通用卡片渲染, 后端是存储权威)
    const ghost = useAppStore.getState().rfNodes.find((n) => n.id === 'w-ghost');
    expect(ghost?.type).toBe('widget');
    expect((ghost?.data as { widget: { kind: string } }).widget.kind).toBe('FutureWidget');
    // 边逐字保留 — 前端不删除后端认可的连线
    expect(useAppStore.getState().rfEdges.some((e) => e.id === 'e-ghost')).toBe(true);
    // 占位控件记录随 widgets 数组收敛, 再次同步时按原样回传
    expect(useAppStore.getState().widgets.some((w) => w.params.id === 'w-ghost')).toBe(true);
  });

  it('事件位置表驱动已有节点位置跟随', () => {
    adoptSourceGraph({
      ...SOURCE_EVENT,
      widgets: [
        {
          id: 'w-gauge',
          kind: 'Gauge',
          params: { id: 'w-gauge', label: 'G', min: 0, max: 100, unit: '', channel: null },
        },
      ],
      positions: {
        'w-gauge': { x: 700, y: 90 },
        'transport-1': { x: 5, y: 6 },
      },
    });

    const { rfNodes } = useAppStore.getState();
    expect(rfNodes.find((n) => n.id === 'w-gauge')?.position).toEqual({ x: 700, y: 90 });
    expect(rfNodes.find((n) => n.id === 'transport-1')?.position).toEqual({ x: 5, y: 6 });
  });
});

describe('syncTabGraphToBackend (端口提示 / 版本冲突重试)', () => {
  beforeEach(() => {
    tauriMock.invoke.mockClear();
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue({ nodes: [] });
    seedStore();
  });

  it('提交附带端口提示与 base_version, 响应版本号写回', async () => {
    (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue({
      nodes: [],
      version: 42,
    });

    await syncTabGraphToBackend('default');

    const call = (tauriMock.invoke.mock.calls as unknown as [string, Record<string, unknown>][]).find(
      (c) => c[0] === 'update_tab_graph'
    );
    expect(call).toBeDefined();
    const args = call![1];
    expect(args.baseVersion).toBe(1);
    const hints = args.nodeHints as Record<string, { default_output?: string; default_input?: string }>;
    expect(hints['transport-1']).toMatchObject({ default_output: 'rx', default_input: 'tx' });
    expect(hints['protocol-1']).toMatchObject({ default_output: 'out', default_input: 'in' });
    expect(hints['w-gauge']).toMatchObject({ default_input: 'value' });
    expect(useAppStore.getState().graphVersion).toBe(42);
  });

  it('版本冲突 → 拉权威源图采纳 → 以新版本重试一次', async () => {
    let graphCalls = 0;
    (tauriMock.invoke as unknown as {
      mockImplementation: (f: (cmd: string, args?: unknown) => unknown) => void;
    }).mockImplementation((cmd: string) => {
      if (cmd === 'update_tab_graph') {
        graphCalls += 1;
        if (graphCalls === 1) {
          // 模拟期间拓扑 op 推进了版本
          return Promise.reject(Object.assign(new Error('图版本冲突: 基线过期 (后端当前 v5)'), {
            kind: 'Config',
            data: { current: '5' },
          }));
        }
        return Promise.resolve({ nodes: [], version: 6 });
      }
      if (cmd === 'get_source_graph') {
        return Promise.resolve({
          tab_id: 'default',
          version: 5,
          nodes: [],
          edges: [
            { id: 'e-byte', source: 'transport-1', source_handle: 'rx', target: 'protocol-1', target_handle: 'in' },
            { id: 'e-remote', source: 'protocol-1', source_handle: 'ch0', target: 'w-gauge', target_handle: 'value' },
          ],
        });
      }
      return Promise.resolve({ nodes: [] });
    });

    const err = await syncTabGraphToBackend('default');
    expect(err).toBeUndefined();
    expect(graphCalls).toBe(2);

    // 重试提交的 baseVersion 已同步为冲突时拉到的版本
    const calls = (tauriMock.invoke.mock.calls as unknown as [string, Record<string, unknown>][]).filter(
      (c) => c[0] === 'update_tab_graph'
    );
    expect(calls[1][1].baseVersion).toBe(5);
    // 远端新增边已采纳进重试载荷
    const edges = calls[1][1].edges as { id: string }[];
    expect(edges.some((e) => e.id === 'e-remote')).toBe(true);
    expect(useAppStore.getState().graphVersion).toBe(6);
  });

  it('编译失败返回用户可读错误文案 (供 AI 工具结果回传)', async () => {
    (tauriMock.invoke as unknown as {
      mockImplementation: (f: (cmd: string) => unknown) => void;
    }).mockImplementation((cmd: string) => {
      if (cmd === 'update_tab_graph') {
        return Promise.reject(Object.assign(
          new Error('图编译失败: 边 e1 端口域不匹配: pt.out (bytes) → m1.in0 (f32)'),
          { kind: 'Config' },
        ));
      }
      return Promise.resolve({ nodes: [] });
    });

    const err = await syncTabGraphToBackend('default');
    expect(err).toContain('域不匹配');
  });
});
