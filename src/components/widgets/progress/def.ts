import { BarChartHorizontal } from 'lucide-react';
import { ProgressBar } from './ProgressBarWidget';
import { ProgressProperties } from './ProgressProperties';
import type { WidgetDef } from '../registryTypes';

/// 进度条控件定义
export const progressDef: WidgetDef<'Progress'> = {
  kind: 'Progress',
  icon: BarChartHorizontal,
  labelKey: 'progressBar',
  Component: ProgressBar,
  Properties: ProgressProperties,
};
