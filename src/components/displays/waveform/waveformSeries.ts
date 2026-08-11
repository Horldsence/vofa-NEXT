/// 波形图 series 共享逻辑 — 主图 (uPlot) 与缩略图 (WaveformTimeline) 共用
/// 保证两边显示的 series 列表、取数方式、颜色完全一致, 避免逻辑分叉
import type { Edge } from '@xyflow/react';
import { CHANNEL_COLORS, DERIVED_COLORS } from './waveformConstants';

/// 波形图连接的输入 — 可以是原始通道或派生节点 (Math/Filter 等)
export type ConnectedInput =
  | { kind: 'channel'; idx: number }
  | { kind: 'derived'; sourceId: string; sourceHandle: string };

/// 系列 slot — 用于 series 创建/数据获取/游标读数/缩略图
/// channelIdx: 通道索引 (用于颜色和 effective channel 配置)
/// derivedIdx: 派生 series 索引 (用于颜色, -1 表示非派生)
export interface SeriesSlot {
  input: ConnectedInput;
  /// 通道颜色索引 (channel 用 idx, derived 用 derivedIdx)
  colorIdx: number;
  /// 是否为派生 series
  isDerived: boolean;
  /// 显示标签 (CH0 / MATH:widgetId)
  label: string;
  /// 用于 effective channel 配置查询的索引
  /// channel: 用 idx; derived: 用 widget.params.channels + derivedIdx
  cfgIdx: number;
}

/// 冻结数据快照 — Stop 时由主图拍快照, 缩略图与导出共用
export interface FrozenWaveformData {
  ts: number[];
  chs: number[][];
  derived?: Record<string, Record<string, number[]>>;
}

/// 缩略图 series 描述 — 主图已按 show 过滤, 缩略图照单绘制
export interface TimelineSeriesSpec {
  input: ConnectedInput;
  /// 用于 effective channel 配置查询的索引 (与主图 slot.cfgIdx 一致)
  cfgIdx: number;
  /// series 颜色 (与主图一致)
  color: string;
}

/// 计算连接到波形图的输入列表 (通道在前, 派生在后)
/// default-waveform 固定显示全部通道; 其它波形图按 rfEdges 连接关系确定
export function computeConnectedInputs(
  widgetId: string,
  widgetChannels: number,
  rfEdges: Edge[]
): ConnectedInput[] {
  if (widgetId === 'default-waveform') {
    return Array.from({ length: widgetChannels }, (_, i) => ({ kind: 'channel' as const, idx: i }));
  }
  const channels: ConnectedInput[] = [];
  const derived: ConnectedInput[] = [];
  for (const e of rfEdges) {
    if (e.target !== widgetId) continue;
    const handle = e.sourceHandle ?? '';
    const m = /^ch(\d+)$/.exec(handle);
    if (m) {
      channels.push({ kind: 'channel', idx: parseInt(m[1], 10) });
    } else {
      derived.push({ kind: 'derived', sourceId: e.source, sourceHandle: handle });
    }
  }
  // 通道在前, 派生在后
  return [...channels, ...derived];
}

/// 由输入列表构建 series slots (主图 series 创建与缩略图绘制共用)
export function buildSeriesSlots(
  connectedInputs: ConnectedInput[],
  widgetChannels: number,
  dynamicSeries: boolean
): SeriesSlot[] {
  const channelInputs = connectedInputs.filter(
    (i): i is Extract<ConnectedInput, { kind: 'channel' }> => i.kind === 'channel'
  );
  const derivedInputs = connectedInputs.filter(
    (i): i is Extract<ConnectedInput, { kind: 'derived' }> => i.kind === 'derived'
  );
  const slots: SeriesSlot[] = [];

  if (dynamicSeries) {
    // 动态: 仅连接的通道 + 派生
    for (const input of channelInputs) {
      slots.push({
        input,
        colorIdx: input.idx,
        isDerived: false,
        label: `CH${input.idx}`,
        cfgIdx: input.idx,
      });
    }
    for (let i = 0; i < derivedInputs.length; i++) {
      const input = derivedInputs[i];
      slots.push({
        input,
        colorIdx: i,
        isDerived: true,
        label: `MATH:${input.sourceId}`,
        cfgIdx: widgetChannels + i,
      });
    }
  } else {
    // 固定: 仅创建已连接通道的系列 (与缩略图一致, 不创建未连接的占位槽)
    for (const input of channelInputs) {
      slots.push({
        input,
        colorIdx: input.idx,
        isDerived: false,
        label: `CH${input.idx}`,
        cfgIdx: input.idx,
      });
    }
    for (let i = 0; i < derivedInputs.length; i++) {
      const input = derivedInputs[i];
      slots.push({
        input,
        colorIdx: channelInputs.length + i,
        isDerived: true,
        label: `MATH:${input.sourceId}`,
        cfgIdx: channelInputs.length + i,
      });
    }
  }
  return slots;
}

/// slot 显示颜色 — 与主图 series 颜色一致
export function slotColor(slot: SeriesSlot): string {
  return slot.isDerived
    ? DERIVED_COLORS[slot.colorIdx % DERIVED_COLORS.length]
    : CHANNEL_COLORS[slot.colorIdx % CHANNEL_COLORS.length];
}

/// 为单个输入从数据源解析原始数据数组 (与 timestamps 对齐, 缺失/长度不符补 NaN)
/// 通道输入: 从 channelArrays[idx] 取; 派生输入: 从 derivedMap[widgetId]?.[sourceId] 取
export function resolveInputArray(
  input: ConnectedInput,
  widgetId: string,
  tsLen: number,
  channelArrays: number[][],
  derivedMap: Record<string, Record<string, number[]>> | undefined
): number[] {
  let arr: number[] | undefined;
  if (input.kind === 'channel') {
    arr = channelArrays[input.idx];
  } else {
    arr = derivedMap?.[widgetId]?.[input.sourceId];
  }
  if (!arr) return Array(tsLen).fill(NaN);
  if (arr.length === tsLen) return arr;
  // 对齐: 截断或补 NaN
  const padded = arr.slice(0, tsLen);
  while (padded.length < tsLen) padded.push(NaN);
  return padded;
}
