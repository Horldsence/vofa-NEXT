import { describe, expect, it } from 'vitest';
import type { Edge } from '@xyflow/react';
import { findInputEdge, isPortConnected, resolveStringSource } from '../stringPorts';

function edge(source: string, sourceHandle: string | null, target: string, targetHandle: string): Edge {
  return { id: `${source}->${target}`, source, sourceHandle, target, targetHandle };
}

describe('stringPorts 边解析', () => {
  const edges: Edge[] = [
    edge('trig-1', 'text', 'disp-1', 'text'),
    edge('knob-1', 'value', 'str-1', 'pos'),
  ];

  it('findInputEdge: 按 (target, targetHandle) 精确匹配', () => {
    expect(findInputEdge(edges, 'disp-1', 'text')?.source).toBe('trig-1');
    expect(findInputEdge(edges, 'str-1', 'pos')?.source).toBe('knob-1');
    // 目标不同 / 端口不同 / 反向 (source 侧) 均不匹配
    expect(findInputEdge(edges, 'disp-1', 'pos')).toBeUndefined();
    expect(findInputEdge(edges, 'other', 'text')).toBeUndefined();
    expect(findInputEdge(edges, 'trig-1', 'text')).toBeUndefined();
  });

  it('isPortConnected: 有入边 true, 无入边 false', () => {
    expect(isPortConnected(edges, 'str-1', 'pos')).toBe(true);
    // len 口无边 → 内联框应启用
    expect(isPortConnected(edges, 'str-1', 'len')).toBe(false);
    expect(isPortConnected([], 'str-1', 'pos')).toBe(false);
  });

  it('resolveStringSource: 有边返回上游地址, 无边返回 null', () => {
    expect(resolveStringSource(edges, 'disp-1', 'text')).toEqual({ source: 'trig-1', handle: 'text' });
    expect(resolveStringSource(edges, 'disp-1', 'missing')).toBeNull();
  });

  it('resolveStringSource: sourceHandle 缺省回退 text (字符串平面常规输出口)', () => {
    const noHandle: Edge[] = [edge('trig-1', null, 'disp-1', 'text')];
    expect(resolveStringSource(noHandle, 'disp-1', 'text')).toEqual({ source: 'trig-1', handle: 'text' });
  });
});
