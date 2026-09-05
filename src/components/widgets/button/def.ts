// ============ 按钮控件定义 ============

import { Square as SquareIcon } from 'lucide-react';
import { ButtonWidget } from './ButtonWidget';
import { ButtonProperties } from './ButtonProperties';
import type { WidgetDef } from '../registryTypes';

/// 按钮控件定义
export const buttonDef: WidgetDef<'Button'> = {
  kind: 'Button',
  icon: SquareIcon,
  labelKey: 'button',
  Component: ButtonWidget,
  Properties: ButtonProperties,
};
