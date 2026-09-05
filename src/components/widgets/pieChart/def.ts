// ============ 饼图控件定义 ============

import { PieChart as PieChartIcon } from 'lucide-react';
import { PieChart } from './PieChartWidget';
import { PieChartProperties } from './PieChartProperties';
import type { WidgetDef } from '../registryTypes';

/// 饼图定义
export const pieChartDef: WidgetDef<'PieChart'> = {
  kind: 'PieChart',
  icon: PieChartIcon,
  labelKey: 'pieChart',
  Component: PieChart,
  Properties: PieChartProperties,
};
