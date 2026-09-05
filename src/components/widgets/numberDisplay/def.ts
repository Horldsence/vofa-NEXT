// ============ 大数字显示控件定义 ============

import { Hash } from 'lucide-react';
import { NumberDisplay } from './NumberDisplayWidget';
import { NumberDisplayProperties } from './NumberDisplayProperties';
import type { WidgetDef } from '../registryTypes';

/// 大数字显示定义
export const numberDisplayDef: WidgetDef<'NumberDisplay'> = {
  kind: 'NumberDisplay',
  icon: Hash,
  labelKey: 'numberDisplay',
  Component: NumberDisplay,
  Properties: NumberDisplayProperties,
};
