// ============ 滤波器控件定义 ============

import { Filter as FilterIcon } from 'lucide-react';
import { FilterWidget } from './FilterWidget';
import { FilterProperties } from './FilterProperties';
import type { WidgetDef } from '../registryTypes';

/// 滤波器定义
export const filterDef: WidgetDef<'Filter'> = {
  kind: 'Filter',
  icon: FilterIcon,
  labelKey: 'filter',
  Component: FilterWidget,
  Properties: FilterProperties,
};
