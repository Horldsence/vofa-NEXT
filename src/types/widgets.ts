// ============ 控件配置 ============

import type {
  MathConfig, FilterConfig, SpectrumConfig, Model3DConfig, WidgetBinding,
} from './common';
import type { CommandConfig, FrameDecoderConfig, TableViewConfig } from './frameDecoder';

export interface KnobConfig {
  id: string;
  label: string;
  min: number;
  max: number;
  step: number;
  default: number;
  binding: WidgetBinding;
}

export interface ButtonConfig {
  id: string;
  label: string;
  press_value: number;
  release_value: number;
  binding: WidgetBinding;
}

export interface RadioConfig {
  id: string;
  label: string;
  options: [string, number][];
  default: number;
  binding: WidgetBinding;
}

export interface CheckboxConfig {
  id: string;
  label: string;
  checked_value: number;
  unchecked_value: number;
  default: boolean;
  binding: WidgetBinding;
}

export interface SliderConfig {
  id: string;
  label: string;
  min: number;
  max: number;
  step: number;
  default: number;
  binding: WidgetBinding;
}

export interface LabelConfig {
  id: string;
  text: string;
  channel: number | null;
}

export interface WaveformConfig {
  id: string;
  channels: number;
  max_points: number;
  visible_channels: boolean[];
  /// 动态 series 开关 — false(默认) 固定 widget.params.channels 通道槽 + 派生槽;
  /// true 时按实际连接数决定 series 数 (未连接通道不显示)
  dynamicSeries?: boolean;
}

export interface PieChartConfig {
  id: string;
  label: string;
  segments: string[];
  channels: number[];
}

export interface ImageConfig {
  id: string;
  label: string;
  width: number;
  height: number;
  format: 'rgb888' | 'rgb565' | 'gray8';
}

/// 仪表盘控件 — 半圆指针式显示单通道实时值
export interface GaugeConfig {
  id: string;
  label: string;
  min: number;
  max: number;
  unit: string;          // 单位后缀, 如 'V' / 'A' / ''
  channel: number | null; // 绑定的输入通道 (null = 不绑定)
}

/// LED 指示灯 — 阈值控制开关色
export interface LEDConfig {
  id: string;
  label: string;
  threshold: number;     // 输入 >= threshold 视为 ON
  on_color: string;      // HEX, 如 '#89d185'
  off_color: string;     // HEX, 如 '#3c3c3c'
  channel: number | null;
}

/// 大数字显示 — 大字号展示单通道数值
export interface NumberDisplayConfig {
  id: string;
  label: string;
  unit: string;
  precision: number;     // 小数位数
  channel: number | null;
}

/// 自定义 JS 控件 — 用户代码在 iframe 沙箱中渲染
/// 代码格式见 src/components/displays/CustomWidget.tsx 顶部注释
export interface CustomConfig {
  id: string;
  label: string;
  code: string;           // JS 源码, 求值后应返回 widget 定义对象
  settings: Record<string, string | number | boolean>; // 用户在设置面板里填写的值
}

// ============ 控件类别 ============

/// 控件类别 — 用于 WidgetPalette 分组与颜色区分
export type WidgetCategory =
  | 'input'      // 数据类 (Knob/Button/Radio/Checkbox/Slider/Command)
  | 'display'    // 显示控件 (Waveform/PieChart/Image/Gauge/LED/NumberDisplay/Label/Spectrum/Model3D)
  | 'math'       // 算术控件 (Math/Filter — 加减乘除/数学函数/滤波)
  | 'custom';    // 自定义控件 (Custom JS)

export type WidgetConfig =
  | { kind: 'Knob'; params: KnobConfig }
  | { kind: 'Button'; params: ButtonConfig }
  | { kind: 'Radio'; params: RadioConfig }
  | { kind: 'Checkbox'; params: CheckboxConfig }
  | { kind: 'Slider'; params: SliderConfig }
  | { kind: 'Label'; params: LabelConfig }
  | { kind: 'Waveform'; params: WaveformConfig }
  | { kind: 'PieChart'; params: PieChartConfig }
  | { kind: 'Image'; params: ImageConfig }
  | { kind: 'Gauge'; params: GaugeConfig }
  | { kind: 'LED'; params: LEDConfig }
  | { kind: 'NumberDisplay'; params: NumberDisplayConfig }
  | { kind: 'Custom'; params: CustomConfig }
  | { kind: 'Math'; params: MathConfig }
  | { kind: 'Filter'; params: FilterConfig }
  | { kind: 'Spectrum'; params: SpectrumConfig }
  | { kind: 'Model3D'; params: Model3DConfig }
  | { kind: 'Command'; params: CommandConfig }
  | { kind: 'FrameDecoder'; params: FrameDecoderConfig }
  | { kind: 'TableView'; params: TableViewConfig };

/// 获取控件所属类别 (用于 palette 分组与着色)
export function getWidgetCategory(kind: WidgetConfig['kind']): WidgetCategory {
  switch (kind) {
    case 'Knob':
    case 'Button':
    case 'Radio':
    case 'Checkbox':
    case 'Slider':
    case 'Command':
      return 'input';
    case 'Waveform':
    case 'PieChart':
    case 'Image':
    case 'Gauge':
    case 'LED':
    case 'NumberDisplay':
    case 'Label':
    case 'Spectrum':
    case 'Model3D':
    case 'TableView':
      return 'display';
    case 'Math':
    case 'Filter':
      return 'math';
    case 'Custom':
      return 'custom';
    case 'FrameDecoder':
      return 'input';
  }
}
