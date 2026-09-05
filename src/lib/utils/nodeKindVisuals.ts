//! 节点种类 → 图标 + 主题色 的共享映射。
//!
//! 图标与 WidgetPalette 抽屉保持同源（同一控件在画布上用什么图标，
//! 历史面板行首就用什么图标）；颜色沿用画布的分类主题色体系：
//! - 控件按 getWidgetCategory 分类: input 蓝 / display 绿 / math 橙 /
//!   string 红 / custom 紫（与 PaletteRow 的底色徽章完全一致）
//! - Transport 节点黄色 Cable、Protocol 节点主题色 Binary
//!   （与 TransportNode / ProtocolNode 头部一致）
//!
//! 消费方: 操作历史面板行首徽章；需要新增种类时只改本文件。

import {
  Activity,
  BarChartHorizontal,
  Binary,
  Box,
  Cable,
  CheckSquare,
  Code2,
  FileText,
  Filter,
  Gauge,
  Hash,
  Image,
  Lightbulb,
  LineChart,
  PieChart,
  Radio,
  ScanText,
  Send,
  Sigma,
  Sliders,
  Square,
  Table,
  Tag,
  TextCursorInput,
  Type,
  Zap,
  type LucideIcon,
} from 'lucide-react';
import {
  getWidgetCategory,
  type WidgetCategory,
  type WidgetConfig,
} from '../../types';
import type { Node } from '@xyflow/react';

/** 控件种类 → 图标（与 WidgetPalette 抽屉内条目一致） */
export const WIDGET_KIND_ICONS: Record<WidgetConfig['kind'], LucideIcon> = {
  Knob: Gauge,
  Button: Square,
  Radio: Radio,
  Checkbox: CheckSquare,
  Slider: Sliders,
  Command: Send,
  FrameDecoder: ScanText,
  Trigger: Zap,
  TextInput: TextCursorInput,
  Waveform: LineChart,
  PieChart: PieChart,
  Image: Image,
  Gauge: Gauge,
  LED: Lightbulb,
  NumberDisplay: Hash,
  Label: Tag,
  Spectrum: Activity,
  Model3D: Box,
  RawData: Activity,
  TextDisplay: FileText,
  TableView: Table,
  Math: Sigma,
  Filter: Filter,
  FFT: Activity,
  IFFT: Activity,
  Str: Type,
  Progress: BarChartHorizontal,
  TextOut: Send,
  Custom: Code2,
};

/** 分类 → 徽章底/前景 token 类（与 PaletteRow 的 categoryTileClass 同款） */
const CATEGORY_TILE_CLS: Record<WidgetCategory, string> = {
  input: 'bg-blue/15 text-blue',
  display: 'bg-green/15 text-green',
  math: 'bg-orange/15 text-orange',
  string: 'bg-red/15 text-red',
  custom: 'bg-purple/15 text-purple',
};

/** 分类 → 纯色小圆点 token 类（连线双端点用） */
const CATEGORY_DOT_CLS: Record<WidgetCategory, string> = {
  input: 'bg-blue',
  display: 'bg-green',
  math: 'bg-orange',
  string: 'bg-red',
  custom: 'bg-purple',
};

// 全局节点（字节平面）固定主题 — 与 TransportNode / ProtocolNode 头部一致
export const TRANSPORT_ICON = Cable;
export const TRANSPORT_TILE_CLS = 'bg-yellow/15 text-yellow';
export const TRANSPORT_DOT_CLS = 'bg-yellow';

export const PROTOCOL_ICON = Binary;
export const PROTOCOL_TILE_CLS = 'bg-accent/15 text-accent';
export const PROTOCOL_DOT_CLS = 'bg-accent';

/** 历史记录引用的一个节点端点 */
export type NodeVisualRef =
  | { kind: 'widget'; widgetKind: WidgetConfig['kind'] }
  | { kind: 'transport' }
  | { kind: 'protocol' };

export interface NodeVisual {
  Icon: LucideIcon | null;
  /** 徽章底/前景类 */
  tileCls: string;
  /** 实心小圆点类 */
  dotCls: string;
}

export function widgetVisualOf(widgetKind: WidgetConfig['kind']): NodeVisual {
  const cat = getWidgetCategory(widgetKind);
  return {
    Icon: WIDGET_KIND_ICONS[widgetKind] ?? null,
    tileCls: CATEGORY_TILE_CLS[cat],
    dotCls: CATEGORY_DOT_CLS[cat],
  };
}

export const TRANSPORT_VISUAL: NodeVisual = {
  Icon: TRANSPORT_ICON,
  tileCls: TRANSPORT_TILE_CLS,
  dotCls: TRANSPORT_DOT_CLS,
};

export const PROTOCOL_VISUAL: NodeVisual = {
  Icon: PROTOCOL_ICON,
  tileCls: PROTOCOL_TILE_CLS,
  dotCls: PROTOCOL_DOT_CLS,
};

/** 无明确归属的中性视觉（多选/画布级操作） */
export const NEUTRAL_VISUAL: NodeVisual = {
  Icon: null,
  tileCls: 'bg-bg-input text-text-secondary',
  dotCls: 'bg-border-subtle',
};

export function nodeVisualOf(ref: NodeVisualRef): NodeVisual {
  switch (ref.kind) {
    case 'transport':
      return TRANSPORT_VISUAL;
    case 'protocol':
      return PROTOCOL_VISUAL;
    case 'widget':
      return widgetVisualOf(ref.widgetKind);
  }
}

/** 画布节点 → 视觉引用；无法归类 (异常节点) 返回 null */
export function nodeRefOf(node: Node | undefined | null): NodeVisualRef | null {
  if (!node) return null;
  if (node.type === 'transport') return { kind: 'transport' };
  if (node.type === 'protocol') return { kind: 'protocol' };
  const widget = node.data?.widget as WidgetConfig | undefined;
  if (widget) return { kind: 'widget', widgetKind: widget.kind };
  return null;
}

/** 节点显示名: 全局节点用 data.label, 控件节点用参数 label → 类型名兜底 */
export function nodeLabelOf(node: Node | undefined | null): string {
  if (!node) return '?';
  const label = node.data?.label;
  if (typeof label === 'string' && label) return label;
  const widget = node.data?.widget as WidgetConfig | undefined;
  const params = widget?.params as { label?: string } | undefined;
  return (params?.label ?? widget?.kind) ?? '?';
}
