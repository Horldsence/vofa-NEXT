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

import { classifyRawDataChannel } from '../rawDataChannel';
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
