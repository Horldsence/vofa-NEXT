import { describe, expect, it } from 'vitest';
import type { Node, Edge } from '@xyflow/react';
import { traceTransportSource } from '../appStoreHelpers';

const TRANSPORT_A: Node = {
  id: 'transport-a',
  type: 'transport',
  position: { x: 0, y: 0 },
  data: { global: true, config: { kind: 'TestData' }, label: 'A' },
};
const TRANSPORT_B: Node = {
  id: 'transport-b',
  type: 'transport',
  position: { x: 0, y: 0 },
  data: { global: true, config: { kind: 'Serial' }, label: 'B' },
};
const PROTOCOL: Node = {
  id: 'protocol-1',
  type: 'protocol',
  position: { x: 0, y: 0 },
  data: { global: true, config: { kind: 'JustFloat' }, convertTo: null, channels: 2, label: 'P' },
};
const FRAME_DECODER: Node = {
  id: 'w-fd',
  type: 'widget',
  position: { x: 0, y: 0 },
  data: { tabId: 'default', widget: { kind: 'FrameDecoder', params: { id: 'w-fd', label: 'FD' } } },
};
const MATH_NODE: Node = {
  id: 'w-math',
  type: 'widget',
  position: { x: 0, y: 0 },
  data: { tabId: 'default', widget: { kind: 'Math', params: { id: 'w-math', label: 'M' } } },
};

const NODES = [TRANSPORT_A, TRANSPORT_B, PROTOCOL, FRAME_DECODER, MATH_NODE];

describe('traceTransportSource (沿字节边上溯发送目标串口)', () => {
  it('起点是 Transport 时直接返回自身 (不受互连边干扰)', () => {
    const edges: Edge[] = [
      // B.rx → A.tx 的互连边 — 从 A.rx 通道上溯必须返回 A 而不是 B
      { id: 'e1', source: 'transport-b', sourceHandle: 'rx', target: 'transport-a', targetHandle: 'tx' },
    ];
    expect(traceTransportSource('transport-a', edges, NODES)).toBe('transport-a');
  });

  it('Protocol 的通道 → 上溯到 rx 连线的 Transport', () => {
    const edges: Edge[] = [
      { id: 'e1', source: 'transport-a', sourceHandle: 'rx', target: 'protocol-1', targetHandle: 'in' },
    ];
    expect(traceTransportSource('protocol-1', edges, NODES)).toBe('transport-a');
  });

  it('FrameDecoder 经 Protocol 链路上溯到 Transport', () => {
    const edges: Edge[] = [
      { id: 'e1', source: 'transport-a', sourceHandle: 'rx', target: 'protocol-1', targetHandle: 'in' },
      { id: 'e2', source: 'protocol-1', sourceHandle: 'out', target: 'w-fd', targetHandle: 'in' },
    ];
    expect(traceTransportSource('w-fd', edges, NODES)).toBe('transport-a');
  });

  it('FrameDecoder 直连 Transport rx', () => {
    const edges: Edge[] = [
      { id: 'e1', source: 'transport-b', sourceHandle: 'rx', target: 'w-fd', targetHandle: 'in' },
    ];
    expect(traceTransportSource('w-fd', edges, NODES)).toBe('transport-b');
  });

  it('纯数值链路上溯不到 Transport 返回 null', () => {
    const edges: Edge[] = [
      { id: 'e1', source: 'protocol-1', sourceHandle: 'ch0', target: 'w-math', targetHandle: 'a' },
    ];
    expect(traceTransportSource('w-math', edges, NODES)).toBeNull();
  });

  it('数值边不参与上溯 (chN/field 口跳过)', () => {
    const edges: Edge[] = [
      // protocol 只有数值入边 (异常情况) — 不应沿 ch0 跳到 math
      { id: 'e1', source: 'w-math', sourceHandle: 'result', target: 'protocol-1', targetHandle: 'ch0' },
    ];
    expect(traceTransportSource('protocol-1', edges, NODES)).toBeNull();
  });
});
