// ============ Widget 注册表 ============
//
// 所有控件 kind 的唯一 kind→实现 分发点: 节点内容组件 / 属性面板编辑器 /
// 图标 / i18n 键。新增控件 = 新建 widgets/<kind>/ 文件夹 (组件 + def) 并在
// 下方登记 — Record<WidgetKind, WidgetDef> 在编译期强制穷举,
// registry 守卫测试 (src/components/widgets/__tests__/registry.test.ts)
// 在运行期兜底默认工厂 / 端口 / 尺寸表的完整性。

import type { ComponentType } from 'react';
import type { WidgetConfig } from '../../types';
import type { WidgetComponentProps, WidgetDef, WidgetKind } from './registryTypes';

export type { WidgetKind } from './registryTypes';

import { knobDef } from './knob/def';
import { sliderDef } from './slider/def';
import { buttonDef } from './button/def';
import { radioDef } from './radio/def';
import { checkboxDef } from './checkbox/def';
import { labelDef } from './label/def';
import { textInputDef } from './textInput/def';
import { gaugeDef } from './gauge/def';
import { progressDef } from './progress/def';
import { ledDef } from './led/def';
import { numberDisplayDef } from './numberDisplay/def';
import { pieChartDef } from './pieChart/def';
import { imageDef } from './image/def';
import { waveformDef } from './waveform/def';
import { spectrumDef } from './spectrum/def';
import { model3dDef } from './model3d/def';
import { customDef } from './custom/def';
import { mathDef } from './math/def';
import { filterDef } from './filter/def';
import { fftDef } from './fft/def';
import { ifftDef } from './ifft/def';
import { commandDef } from './command/def';
import { frameDecoderDef } from './frameDecoder/def';
import { rawDataDef } from './rawData/def';
import { triggerDef } from './trigger/def';
import { tableViewDef } from './tableView/def';
import { textDisplayDef } from './textDisplay/def';
import { strDef } from './str/def';
import { textOutDef } from './textOut/def';

/// 编译期穷举的控件定义表 — 单一 kind→实现 权威
export const WIDGET_REGISTRY: { readonly [K in WidgetKind]: WidgetDef<K> } = {
  Knob: knobDef,
  Slider: sliderDef,
  Button: buttonDef,
  Radio: radioDef,
  Checkbox: checkboxDef,
  Label: labelDef,
  TextInput: textInputDef,
  Gauge: gaugeDef,
  Progress: progressDef,
  LED: ledDef,
  NumberDisplay: numberDisplayDef,
  PieChart: pieChartDef,
  Image: imageDef,
  Waveform: waveformDef,
  Spectrum: spectrumDef,
  Model3D: model3dDef,
  Custom: customDef,
  Math: mathDef,
  Filter: filterDef,
  FFT: fftDef,
  IFFT: ifftDef,
  Command: commandDef,
  FrameDecoder: frameDecoderDef,
  RawData: rawDataDef,
  Trigger: triggerDef,
  TableView: tableViewDef,
  TextDisplay: textDisplayDef,
  Str: strDef,
  TextOut: textOutDef,
};

/// 全部控件定义 (registry 顺序 = palette 展示顺序的缺省)
export const WIDGET_DEFS: readonly WidgetDef<WidgetKind>[] = Object.values(WIDGET_REGISTRY) as readonly WidgetDef<WidgetKind>[];

/// 取控件定义 (未知 kind 抛错 — 调用方应先经 normalizeWidgetConfig 保证 kind 合法)
export function getWidgetDef<K extends WidgetKind>(kind: K): WidgetDef<K> {
  return WIDGET_REGISTRY[kind];
}

/// 运行时渲染入口 — 擦除泛型后的组件类型 (WidgetNode / 数据窗口宿主用)
export function widgetComponent(kind: WidgetKind): ComponentType<WidgetComponentProps> {
  return WIDGET_REGISTRY[kind].Component as ComponentType<WidgetComponentProps>;
}

/// 收窄工具 — Extract<WidgetConfig, { kind: K }>
export type WidgetOf<K extends WidgetKind> = Extract<WidgetConfig, { kind: K }>;
