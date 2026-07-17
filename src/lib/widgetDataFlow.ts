/// 控件数据流管理 — 计算所有 widget 的输出值并缓存
///
/// 数据流:
///   ChannelSource (ch0, ch1, ...) ─┐
///   Input Widgets (Knob, Slider) ──┤── Math Widget ─── Display Widgets (Gauge, LED, NumberDisplay, Custom)
///                                  │
///   其他 Math Widget 输出 ─────────┘
///
/// 本模块按拓扑顺序计算每个 widget 的输出:
///   1. ChannelSource 输出 = waveformWindow.channels[chIdx].last
///   2. Input Widget 输出 = 其当前值 (从组件状态同步到 cache)
///   3. Math Widget 输出 = computeMathResult(op, inputs) (递归读取上游)
///   4. Custom Widget 输出 = 由 iframe postMessage 回传 (异步, 不在此处计算)
///
/// 下游 widget 通过 readInputValue(edge) 读取上游输出

import type { Edge, Node } from '@xyflow/react';
import type { WidgetConfig } from '../types';
import { computeMathResult } from '../types';
import { waveformWindow } from './dataBuffer';

/// 缓存: widgetId -> portId -> value
export type WidgetOutputCache = Record<string, Record<string, number>>;

/// 从单条 edge 读取上游输出值
/// - ChannelSource: sourceHandle 形如 "ch0", 直接读 waveformWindow
/// - 其他 widget: 从 cache 读取 widgetId + sourceHandle
export function readInputValue(
  edge: Pick<Edge, 'source' | 'sourceHandle'>,
  cache: WidgetOutputCache
): number {
  const sourceId = edge.source;
  const sourceHandle = edge.sourceHandle ?? 'value';

  // 1. ChannelSource: sourceHandle 形如 "ch0"
  const chMatch = /^ch(\d+)$/.exec(sourceHandle);
  if (chMatch) {
    const chIdx = parseInt(chMatch[1], 10);
    const win = waveformWindow.get();
    const ch = win.channels[chIdx];
    return ch && ch.length > 0 ? ch[ch.length - 1] : 0;
  }

  // 2. 其他 widget 输出: 从 cache 读取
  return cache[sourceId]?.[sourceHandle] ?? 0;
}

/// 读取某 widget 的所有输入 (按 input port id 索引)
export function readAllInputs(
  widgetId: string,
  edges: Edge[],
  cache: WidgetOutputCache
): Record<string, number> {
  const inputs: Record<string, number> = {};
  for (const edge of edges) {
    if (edge.target !== widgetId) continue;
    const portId = edge.targetHandle ?? 'value';
    inputs[portId] = readInputValue(edge, cache);
  }
  return inputs;
}

/// 计算单个 widget 的输出 (不依赖其他 widget 的实时计算结果, 只读 cache)
/// 对于 Math widget, 递归读取上游 (由于已按拓扑顺序处理, 上游在 cache 中已有值)
export function computeWidgetOutput(
  widget: WidgetConfig,
  edges: Edge[],
  cache: WidgetOutputCache
): Record<string, number> {
  switch (widget.kind) {
    case 'Math': {
      const inputs = readAllInputs(widget.params.id, edges, cache);
      const inputArr = Object.values(inputs);
      const result = computeMathResult(widget.params.op, inputArr);
      return { result };
    }
    case 'Knob':
    case 'Slider':
    case 'Button':
    case 'Radio':
    case 'Checkbox': {
      // 这些控件的输出由组件本身通过 setWidgetOutput 同步到 cache
      // 此处返回 cache 中已有值 (不重新计算)
      return cache[widget.params.id] ?? { value: 0 };
    }
    case 'Custom':
      // Custom widget 的输出由 iframe 异步回传, 此处直接返回 cache
      return cache[widget.params.id] ?? {};
    default:
      // 显示控件 (Waveform/PieChart/Image/Gauge/LED/NumberDisplay/Label) 没有输出端口
      return {};
  }
}

/// 拓扑排序 widgets — 按数据依赖顺序排列
/// 仅对有输出端口的 widget 排序 (Math 等), 其他直接放在最后
export function topologicalSort(
  widgets: WidgetConfig[],
  edges: Edge[]
): WidgetConfig[] {
  const widgetIds = new Set(widgets.map((w) => w.params.id));
  const visited = new Set<string>();
  const result: WidgetConfig[] = [];
  const widgetMap = new Map(widgets.map((w) => [w.params.id, w] as const));

  const visit = (id: string, path: Set<string>) => {
    if (visited.has(id)) return;
    if (path.has(id)) {
      // 检测到循环 — 跳过避免死循环
      return;
    }
    const widget = widgetMap.get(id);
    if (!widget) return;

    const nextPath = new Set(path);
    nextPath.add(id);

    // 先访问上游依赖
    for (const edge of edges) {
      if (edge.target !== id) continue;
      const sourceId = edge.source;
      if (widgetIds.has(sourceId) && !visited.has(sourceId)) {
        visit(sourceId, nextPath);
      }
    }

    visited.add(id);
    result.push(widget);
  };

  for (const w of widgets) {
    visit(w.params.id, new Set());
  }

  return result;
}

/// 计算所有 widgets 的输出并返回新 cache
export function computeAllOutputs(
  widgets: WidgetConfig[],
  edges: Edge[],
  prevCache: WidgetOutputCache
): WidgetOutputCache {
  const sorted = topologicalSort(widgets, edges);
  const next: WidgetOutputCache = { ...prevCache };

  for (const widget of sorted) {
    const out = computeWidgetOutput(widget, edges, next);
    next[widget.params.id] = out;
  }

  return next;
}

/// React Flow 节点数据类型 (用于读取 widget 参数)
export type RfNodeLike = Node & { data?: { widget?: WidgetConfig; tabId?: string } };
