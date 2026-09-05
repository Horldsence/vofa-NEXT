//! 连线规则统一模块 — 端口表 / 域解析 / 连线校验的单一权威
//!
//! 消费方 (层级自上而下, 全部同源):
//! - `NodeEditor.isValidConnection` — 手动拖拽的交互校验 (toast 提示)
//! - `toolHost.get_workspace` — 暴露给 AI 的节点端口表 (含 domain)
//! - `adoptSourceGraph` — 画布收敛时的边 handle 存在性校验
//!
//! 校验的最终权威是后端编译器 (DomainMismatch);本模块是同域规则的前端
//! 同构实现, 让无效连线在交互层即刻被拦截、AI 在读取层能自查。

import type { Node } from '@xyflow/react';
import type { DomainType, WidgetConfig } from '../../types';
import { getWidgetPorts, type WidgetPort } from '../../components/nodes/WidgetPorts';
import { isRawDataPreset, protocolPortNames } from './protocolSchema';
import type { ProtocolNodeData } from '../../store/appStoreHelpers';

/// 端口信息 — id + 域 (与 WidgetPort 对齐, 供 get_workspace 直接序列化)
export interface PortInfo {
  id: string;
  domain: DomainType;
}

/// 节点端口表 — inputs / outputs 统一形态 (transport/protocol/widget 通吃)
export interface NodePortTable {
  inputs: PortInfo[];
  outputs: PortInfo[];
}

/// 后端派生端口 (graph:derived 单一权威) 的前端形态
export interface DerivedPort {
  name: string;
  domain: 'F32' | 'Bytes' | 'String' | 'Spectrum';
}

/** 后端派生域 → 前端 DomainType (F32 即时域) */
function derivedDomain(d: DerivedPort['domain']): DomainType {
  return d === 'Spectrum' ? 'freq' : d === 'Bytes' ? 'bytes' : d === 'String' ? 'string' : 'time';
}

/** Protocol 节点数值/字符串端口表 (derivedPorts 优先, protocolPortNames 兜底) */
function protocolValuePorts(data: ProtocolNodeData, derived: DerivedPort[] | undefined, detectedChannels: number | null): PortInfo[] {
  if (derived && derived.length > 0) {
    return derived.map((p) => ({ id: p.name, domain: derivedDomain(p.domain) }));
  }
  const rawData = isRawDataPreset(data);
  return protocolPortNames(data, detectedChannels).map((name) => ({
    id: name,
    domain: rawData && name === 'str' ? 'string' : 'time',
  }));
}

/**
 * 任意节点的统一端口表 (连线校验与 AI 端口暴露的同一数据源)
 *
 * - transport: rx (输出/字节) / tx (输入/字节)
 * - protocol: in (输入/字节) / out (输出/字节) + 数值口 ch0..chN (时域) 或
 *   RawData 预设的 str (字符串域) — 数值口读 derivedPorts, 兜底 protocolPortNames
 * - widget: getWidgetPorts 静态表; RawData 的动态 `src:` 输入口不在表中
 *   (连线时由后端/前端自动改写, 见 validateConnection 的 RawData 特判)
 */
export function nodePortTable(
  node: Node,
  ctx: { derivedPorts?: Record<string, { ports?: DerivedPort[] }>; detectedChannels?: Record<string, number | null> }
): NodePortTable {
  if (node.type === 'transport') {
    return {
      inputs: [{ id: 'tx', domain: 'bytes' }],
      outputs: [{ id: 'rx', domain: 'bytes' }],
    };
  }
  if (node.type === 'protocol') {
    const data = node.data as unknown as ProtocolNodeData;
    const derived = ctx.derivedPorts?.[node.id]?.ports;
    const detected = ctx.detectedChannels?.[node.id] ?? null;
    return {
      inputs: [{ id: 'in', domain: 'bytes' }],
      outputs: [{ id: 'out', domain: 'bytes' }, ...protocolValuePorts(data, derived, detected)],
    };
  }
  const widget = node.data?.widget as WidgetConfig | undefined;
  if (!widget) return { inputs: [], outputs: [] };
  if (widget.kind === 'RawData') {
    // 动态派生口不可静态枚举 — 输出空表; 校验走 RawData 特判 (接收 bytes/time, 拒 freq)
    return { inputs: [], outputs: [] };
  }
  const ports = getWidgetPorts(widget);
  const of = (list: WidgetPort[]): PortInfo[] => list.map((p) => ({ id: p.id, domain: p.domain }));
  return { inputs: of(ports.inputs), outputs: of(ports.outputs) };
}

/**
 * 解析某节点指定端口的域 — 查不到返回 null (视为不校验)
 * (方向仅影响 protocol 节点 in/out 的归属判定)
 */
export function resolvePortDomain(
  node: Node,
  handleId: string | null | undefined,
  direction: 'source' | 'target',
  ctx: Parameters<typeof nodePortTable>[1]
): DomainType | null {
  if (!handleId) return null;
  if (node.type === 'protocol' && (handleId === 'in' || handleId === 'out')) {
    return 'bytes';
  }
  const widget = node.data?.widget as WidgetConfig | undefined;
  // FrameDecoder 旧版回环字节输入口 (兼容旧图数据; 不入端口表 — 新图不再渲染)
  if (widget?.kind === 'FrameDecoder' && direction === 'target' && handleId === 'loopbackIn') {
    return 'bytes';
  }
  const table = nodePortTable(node, ctx);
  const list = direction === 'source' ? table.outputs : table.inputs;
  return list.find((p) => p.id === handleId)?.domain ?? null;
}

/** 连线校验结果 — ok=false 时 message 面向 AI/日志 (中文, 含修正指引) */
export interface ConnectionCheck {
  ok: boolean;
  message?: string;
}

export interface ConnectionCandidate {
  source: string;
  target: string;
  sourceHandle?: string | null;
  targetHandle?: string | null;
}

/** 连线校验上下文 — 节点集合 + 派生端口表 (调用方从 store 组装) */
export interface ConnectionContext {
  nodes: Node[];
  derivedPorts?: Record<string, { ports?: DerivedPort[] }>;
  detectedChannels?: Record<string, number | null>;
}

/**
 * 连线校验 (单一权威的前端同构实现):
 * 1. 两端节点存在
 * 2. widget 节点必须同 tab (全局节点无 tab 约束)
 * 3. 端口存在 (RawData 目标除外 — 动态 `src:` 口自动改写)
 * 4. 域匹配 (时域/频域/字节域/字符串同域; RawData 是 bytes/time 双域 Sink, 仅拒 freq)
 */
export function validateConnection(ctx: ConnectionContext, conn: ConnectionCandidate): ConnectionCheck {
  const source = ctx.nodes.find((n) => n.id === conn.source);
  if (!source) {
    return { ok: false, message: `节点不存在: ${conn.source} (先 get_workspace 查询有效 id)` };
  }
  const target = ctx.nodes.find((n) => n.id === conn.target);
  if (!target) {
    return { ok: false, message: `节点不存在: ${conn.target} (先 get_workspace 查询有效 id)` };
  }

  // widget 必须同 tab (全局节点渲染在所有画布, 无 tab 归属)
  const srcTab = source.data?.tabId as string | undefined;
  const tgtTab = target.data?.tabId as string | undefined;
  if (srcTab && tgtTab && srcTab !== tgtTab) {
    return { ok: false, message: `跨 tab 连线: 源在 ${srcTab}, 目标在 ${tgtTab} (控件只能与同页控件或全局节点相连)` };
  }

  const sourceDomain = resolvePortDomain(source, conn.sourceHandle, 'source', ctx);
  if (!conn.sourceHandle || sourceDomain === null) {
    const outputs = nodePortTable(source, ctx).outputs;
    return {
      ok: false,
      message: `节点 ${conn.source} 没有输出端口 ${conn.sourceHandle ?? '(未指定)'} (可选: ${outputs.map((p) => p.id).join('/') || '无'})`,
    };
  }

  const targetWidget = target.data?.widget as WidgetConfig | undefined;
  const isRawDataTarget = targetWidget?.kind === 'RawData';

  if (!isRawDataTarget) {
    const targetDomain = resolvePortDomain(target, conn.targetHandle, 'target', ctx);
    if (!conn.targetHandle || targetDomain === null) {
      const inputs = nodePortTable(target, ctx).inputs;
      return {
        ok: false,
        message: `节点 ${conn.target} 没有输入端口 ${conn.targetHandle ?? '(未指定)'} (可选: ${inputs.map((p) => p.id).join('/') || '无'})`,
      };
    }
    if (sourceDomain !== targetDomain) {
      return {
        ok: false,
        message: `端口域不匹配: ${conn.source}.${conn.sourceHandle} (${sourceDomain}) → ${conn.target}.${conn.targetHandle} (${targetDomain})。同域端口才能相连 (time/freq/bytes/string)`,
      };
    }
  } else if (sourceDomain === 'freq') {
    // RawData 是 bytes/time 双域 Sink (通道显示原始字节流), 仅频域源被拒
    return {
      ok: false,
      message: `端口域不匹配: 频域源 ${conn.source}.${conn.sourceHandle} (freq) 不能直连 RawData 控件 (仅接受时域/字节域)`,
    };
  }

  return { ok: true };
}

/**
 * 校验既有边的 handle 是否仍存在 (画布收敛时剔除悬空边)
 * RawData 的 `src:` 目标口按派生约定放行
 */
export function edgeHandlesValid(ctx: ConnectionContext, edge: { source: string; sourceHandle?: string | null; target: string; targetHandle?: string | null }): boolean {
  const source = ctx.nodes.find((n) => n.id === edge.source);
  const target = ctx.nodes.find((n) => n.id === edge.target);
  if (!source || !target) return true; // 端点不存在交给节点删除逻辑, 不在此判定
  if (resolvePortDomain(source, edge.sourceHandle, 'source', ctx) === null) return false;
  const targetWidget = target.data?.widget as WidgetConfig | undefined;
  if (targetWidget?.kind === 'RawData' && (edge.targetHandle ?? '').startsWith('src:')) return true;
  if (resolvePortDomain(target, edge.targetHandle, 'target', ctx) === null) return false;
  return true;
}
