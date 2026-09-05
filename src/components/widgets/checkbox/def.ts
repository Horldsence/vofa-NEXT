// ============ 复选控件定义 ============

import { CheckSquare as CheckSquareIcon } from 'lucide-react';
import { Checkbox } from './CheckboxWidget';
import { CheckboxProperties } from './CheckboxProperties';
import type { WidgetDef } from '../registryTypes';

/// 复选控件定义
export const checkboxDef: WidgetDef<'Checkbox'> = {
  kind: 'Checkbox',
  icon: CheckSquareIcon,
  labelKey: 'checkbox',
  Component: Checkbox,
  Properties: CheckboxProperties,
};
