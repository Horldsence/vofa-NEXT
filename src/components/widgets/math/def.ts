// ============ 算术控件定义 ============

import { Sigma } from 'lucide-react';
import { MathWidget } from './MathWidget';
import { MathProperties } from './MathProperties';
import type { WidgetDef } from '../registryTypes';

/// 算术控件定义
export const mathDef: WidgetDef<'Math'> = {
  kind: 'Math',
  icon: Sigma,
  labelKey: 'math',
  Component: MathWidget,
  Properties: MathProperties,
};
