// ============ LED 控件定义 ============

import { Lightbulb } from 'lucide-react';
import { LED } from './LEDWidget';
import { LEDProperties } from './LEDProperties';
import type { WidgetDef } from '../registryTypes';

/// LED 指示灯定义
export const ledDef: WidgetDef<'LED'> = {
  kind: 'LED',
  icon: Lightbulb,
  labelKey: 'led',
  Component: LED,
  Properties: LEDProperties,
};
