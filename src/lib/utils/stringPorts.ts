/// 字符串平面端口解析 — 与 useGraphInput 的数值平面边解析同构
///
/// 两层输出平面:
/// - 数值平面: graphOutputs[widgetId][portId] (f32)
/// - 字符串平面: customTextOutputs[widgetId][portId] (string)
///
/// 本模块只负责「边 → 读取地址」的纯函数解析, 供 StrWidget / TextDisplay 等
/// 字符串消费者使用; 数值平面仍走 useGraphInput / useGraphInputs。

import type { Edge } from '@xyflow/react';

/// 查找连到指定节点输入端口的边 (React Flow: target / targetHandle)
export function findInputEdge(edges: Edge[], nodeId: string, portId: string): Edge | undefined {
  return edges.find((e) => e.target === nodeId && e.targetHandle === portId);
}

/// 端口是否已有入边 — StrWidget 数值内联框据此切换 可编辑(无边) / 只读展示上游值(有边)
export function isPortConnected(edges: Edge[], nodeId: string, portId: string): boolean {
  return findInputEdge(edges, nodeId, portId) !== undefined;
}

/// 解析字符串输入端口的上游读取地址; 无入边返回 null (调用方走回退路径)
///
/// handle 缺省 'text' — 字符串平面的常规输出口 (Trigger.text);
/// 实连的边都带显式 sourceHandle, 缺省仅兜底
export function resolveStringSource(
  edges: Edge[],
  nodeId: string,
  portId: string
): { source: string; handle: string } | null {
  const edge = findInputEdge(edges, nodeId, portId);
  if (!edge) return null;
  return { source: edge.source, handle: edge.sourceHandle ?? 'text' };
}
