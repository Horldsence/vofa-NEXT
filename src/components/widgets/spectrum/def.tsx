// ============ 频谱展示控件定义 ============
//
// 纯窗口型展示 — 节点内仅占位提示, 实际绘制在数据窗口 (无组件文件)。

import { Activity } from 'lucide-react';
import { SpectrumPlaceholder } from './SpectrumPlaceholder';
import type { WidgetDef } from '../registryTypes';

/// 频谱展示定义
export const spectrumDef: WidgetDef<'Spectrum'> = {
  kind: 'Spectrum',
  icon: Activity,
  labelKey: 'spectrum',
  Component: SpectrumPlaceholder,
};
