import { describe, expect, it, vi } from 'vitest';

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

import { classifyRawDataChannel, resolveRawDataChannelKey, resolveRawDataStatusTransport } from '../rawDataChannel';
import type { Node, Edge } from '@xyflow/react';
import type { WidgetConfig } from '../../../types';

const TRANSPORT_NODE: Node = {
  id: 'transport-1',
  type: 'transport',
  position: { x: 0, y: 0 },
  data: { global: true, config: { kind: 'Serial', params: {} }, label: 'Serial' },
};

const PROTOCOL_NODE: Node = {
  id: 'protocol-1',
  type: 'protocol',
  position: { x: 0, y: 0 },
  data: { global: true, config: { kind: 'RawData' }, convertTo: null, label: 'RawData' },
};

const DECODER_WIDGET = {
  kind: 'FrameDecoder',
  params: { id: 'w-dec', label: 'dec' },
} as unknown as WidgetConfig;

const BYTE_EDGE: Edge = {
  id: 'e-byte',
  source: 'transport-1',
  sourceHandle: 'rx',
  target: 'protocol-1',
  targetHandle: 'in',
};

describe('classifyRawDataChannel', () => {
  it('Transport rx 源 → byte-source, transportId 为源节点本身', () => {
    const info = classifyRawDataChannel(
      { sourceId: 'transport-1', sourceHandle: 'rx' },
      [TRANSPORT_NODE],
      [],
      []
    );
    expect(info).toEqual({ kind: 'byte-source', transportId: 'transport-1' });
  });

  it('Protocol out 源 (已连 Transport) → byte-source, transportId 沿字节边上溯', () => {
    const info = classifyRawDataChannel(
      { sourceId: 'protocol-1', sourceHandle: 'out' },
      [TRANSPORT_NODE, PROTOCOL_NODE],
      [BYTE_EDGE],
      []
    );
    expect(info).toEqual({ kind: 'byte-source', transportId: 'transport-1' });
  });

  it('Protocol out 源 (未连 Transport) → byte-source, transportId = null', () => {
    const info = classifyRawDataChannel(
      { sourceId: 'protocol-1', sourceHandle: 'out' },
      [PROTOCOL_NODE],
      [],
      []
    );
    expect(info).toEqual({ kind: 'byte-source', transportId: null });
  });

  it('Protocol chN 数值口 → numeric (解析后的 f32 采样, 非原始字节流)', () => {
    const info = classifyRawDataChannel(
      { sourceId: 'protocol-1', sourceHandle: 'ch0' },
      [TRANSPORT_NODE, PROTOCOL_NODE],
      [BYTE_EDGE],
      []
    );
    expect(info).toEqual({ kind: 'numeric', transportId: null });
  });

  it('Protocol str 口 (RawData 预设字符串行) → byte-source', () => {
    const info = classifyRawDataChannel(
      { sourceId: 'protocol-1', sourceHandle: 'str' },
      [TRANSPORT_NODE, PROTOCOL_NODE],
      [BYTE_EDGE],
      []
    );
    expect(info).toEqual({ kind: 'byte-source', transportId: 'transport-1' });
  });

  it('FrameDecoder 的 raw 口 → decoder-node', () => {
    const info = classifyRawDataChannel(
      { sourceId: 'w-dec', sourceHandle: 'raw' },
      [],
      [],
      [DECODER_WIDGET]
    );
    expect(info).toEqual({ kind: 'decoder-node', transportId: null });
  });

  it('普通数值源 (widget 输出) → numeric', () => {
    const info = classifyRawDataChannel(
      { sourceId: 'w-math', sourceHandle: 'result' },
      [],
      [],
      [DECODER_WIDGET]
    );
    expect(info).toEqual({ kind: 'numeric', transportId: null });
  });

  it('FrameDecoder 的非 raw 口 → numeric', () => {
    const info = classifyRawDataChannel(
      { sourceId: 'w-dec', sourceHandle: 'value' },
      [],
      [],
      [DECODER_WIDGET]
    );
    expect(info).toEqual({ kind: 'numeric', transportId: null });
  });
});

describe('resolveRawDataChannelKey - 纯端口制选择解析', () => {
  const OPTIONS = [
    { key: 'src:transport-1:rx' },
    { key: 'src:protocol-1:out' },
    { key: 'src:w-math:result' },
  ];

  it('配置选中且连线仍存在 → 保持该卡片的独立选择', () => {
    expect(resolveRawDataChannelKey('src:protocol-1:out', OPTIONS)).toBe('src:protocol-1:out');
  });

  it('无配置 (旧数据) → 回退第一个已连接端口', () => {
    expect(resolveRawDataChannelKey(undefined, OPTIONS)).toBe('src:transport-1:rx');
    expect(resolveRawDataChannelKey('', OPTIONS)).toBe('src:transport-1:rx');
  });

  it('配置失效 (连线已删除) → 回退第一个已连接端口', () => {
    expect(resolveRawDataChannelKey('src:gone:rx', OPTIONS)).toBe('src:transport-1:rx');
  });

  it('无任何连线 → null (视图渲染空态引导)', () => {
    expect(resolveRawDataChannelKey('src:transport-1:rx', [])).toBeNull();
    expect(resolveRawDataChannelKey(undefined, [])).toBeNull();
  });

  it('不同卡片配置互不影响 (各自解析各自的 key)', () => {
    // 卡片 A 选中 protocol out, 卡片 B 无配置 — 同一 options 下各自成立
    expect(resolveRawDataChannelKey('src:protocol-1:out', OPTIONS)).toBe('src:protocol-1:out');
    expect(resolveRawDataChannelKey(undefined, OPTIONS.slice(1))).toBe('src:protocol-1:out');
  });
});

describe('resolveRawDataStatusTransport - 卡片状态提示的可观察 Transport', () => {
  const RAW_WIDGET = {
    kind: 'RawData',
    params: { id: 'w-raw', label: 'Raw' },
  } as unknown as WidgetConfig;

  // w-raw 的入边: transport rx 直连 + protocol out (两条候选通道)
  // 另含 protocol 的上游字节边 (transport.rx → protocol.in) — 状态提示需沿边上溯
  const CARD_EDGES: Edge[] = [
    BYTE_EDGE,
    {
      id: 'e1',
      source: 'transport-1',
      sourceHandle: 'rx',
      target: 'w-raw',
      targetHandle: 'src:transport-1:rx',
    },
    {
      id: 'e2',
      source: 'protocol-1',
      sourceHandle: 'out',
      target: 'w-raw',
      targetHandle: 'src:protocol-1:out',
    },
  ];
  const CARD_NODES = [TRANSPORT_NODE, PROTOCOL_NODE];

  it('默认 (无 selectedInput) → 第一个入边的源 Transport', () => {
    expect(
      resolveRawDataStatusTransport('w-raw', undefined, CARD_EDGES, CARD_NODES, [RAW_WIDGET])
    ).toBe('transport-1');
  });

  it('配置选中 protocol out → 沿字节边上溯到 Transport', () => {
    expect(
      resolveRawDataStatusTransport(
        'w-raw',
        'src:protocol-1:out',
        CARD_EDGES,
        CARD_NODES,
        [RAW_WIDGET]
      )
    ).toBe('transport-1');
  });

  it('配置失效 (连线已删) → 回退第一个入边', () => {
    expect(
      resolveRawDataStatusTransport(
        'w-raw',
        'src:gone:rx',
        CARD_EDGES,
        CARD_NODES,
        [RAW_WIDGET]
      )
    ).toBe('transport-1');
  });

  it('FrameDecoder raw 口 → null (节点旁路, 无固定连接语义)', () => {
    const decEdges: Edge[] = [
      {
        id: 'e3',
        source: 'w-dec',
        sourceHandle: 'raw',
        target: 'w-raw',
        targetHandle: 'src:w-dec:raw',
      },
    ];
    expect(
      resolveRawDataStatusTransport('w-raw', 'src:w-dec:raw', decEdges, [], [
        RAW_WIDGET,
        DECODER_WIDGET,
      ])
    ).toBeNull();
  });

  it('无任何连线 → null (空态, 无状态可提示)', () => {
    expect(
      resolveRawDataStatusTransport('w-raw', undefined, [], CARD_NODES, [RAW_WIDGET])
    ).toBeNull();
  });
});
