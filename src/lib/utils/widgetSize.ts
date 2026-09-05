// ============ 控件节点尺寸 ============
//
// 控件节点 (React Flow widget 节点) 的尺寸约束单一权威:
// - 未显式调整过大小的节点保持内容自适应 (节点上不设 width/height);
// - 用户拖拽 NodeResizer 或在属性面板填入尺寸后, 显式尺寸随 position 一起持久化到后端;
// - 拖拽/输入都被钳制在每类控件的最小尺寸与全局上限之间。

import type { WidgetConfig } from '../../types';

export interface WidgetSizeLimits {
  minW: number;
  minH: number;
}

export interface WidgetSize {
  width?: number;
  height?: number;
}

/// 新拖出控件的初始显式宽度 (height 缺省 = 随内容自适应)。
/// 取值在旧 CSS 钳制区间 (160–240px) 内, 保持既有观感。
export const WIDGET_DEFAULT_WIDTH = 200;

/// 全局尺寸上限 — 防止误拖出夸张尺寸 (画布可 zoom, 上限宽松即可)
const MAX_W = 1600;
const MAX_H = 1200;

/// 兜底约束 (未知 kind / 未单列的控件)
const FALLBACK_LIMITS: WidgetSizeLimits = { minW: 140, minH: 80 };

/// 每类控件的最小尺寸 — 以标题栏 + 内容在默认字号下的可读下限为基准;
/// 占位型节点 (实际渲染在数据窗口, 节点内只有占位文字) 用统一的紧凑下限。
const PLACEHOLDER_LIMITS: WidgetSizeLimits = { minW: 140, minH: 80 };

export const WIDGET_SIZE_LIMITS: Record<WidgetConfig['kind'], WidgetSizeLimits> = {
  // 输入控件
  Knob: { minW: 140, minH: 96 },
  Slider: { minW: 140, minH: 72 },
  Button: { minW: 120, minH: 64 },
  Radio: { minW: 140, minH: 72 },
  Checkbox: { minW: 140, minH: 72 },
  Command: PLACEHOLDER_LIMITS,
  Trigger: PLACEHOLDER_LIMITS,
  TextInput: { minW: 160, minH: 72 },
  // 显示控件
  Label: { minW: 100, minH: 48 },
  Waveform: PLACEHOLDER_LIMITS,
  Spectrum: PLACEHOLDER_LIMITS,
  Model3D: PLACEHOLDER_LIMITS,
  PieChart: { minW: 160, minH: 140 },
  Image: PLACEHOLDER_LIMITS,
  Gauge: { minW: 160, minH: 128 },
  Progress: { minW: 140, minH: 80 },
  LED: { minW: 120, minH: 80 },
  NumberDisplay: { minW: 140, minH: 80 },
  RawData: PLACEHOLDER_LIMITS,
  TableView: PLACEHOLDER_LIMITS,
  TextDisplay: { minW: 140, minH: 64 },
  TextOut: { minW: 150, minH: 96 },
  // 算术 / 字符串控件
  Math: { minW: 150, minH: 90 },
  Filter: { minW: 150, minH: 90 },
  FFT: { minW: 150, minH: 90 },
  IFFT: { minW: 150, minH: 90 },
  Str: { minW: 150, minH: 90 },
  // 自定义 / 协议
  Custom: { minW: 160, minH: 120 },
  FrameDecoder: PLACEHOLDER_LIMITS,
};

export function widgetMinSize(kind: WidgetConfig['kind']): WidgetSizeLimits {
  return WIDGET_SIZE_LIMITS[kind] ?? FALLBACK_LIMITS;
}

/// 把用户输入/拖拽的尺寸钳制进该控件的合法区间;
/// undefined (自适应) 原样保留。
export function clampWidgetSize(kind: WidgetConfig['kind'], size: WidgetSize): WidgetSize {
  const { minW, minH } = widgetMinSize(kind);
  const clamp = (v: number | undefined, min: number, max: number) =>
    v === undefined ? undefined : Math.min(max, Math.max(min, Math.round(v)));
  return {
    width: clamp(size.width, minW, MAX_W),
    height: clamp(size.height, minH, MAX_H),
  };
}
