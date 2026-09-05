import { Gauge as GaugeIcon } from 'lucide-react';
import { Knob } from './KnobWidget';
import { KnobProperties } from './KnobProperties';
import type { WidgetDef } from '../registryTypes';

/// 旋钮控件定义
export const knobDef: WidgetDef<'Knob'> = {
  kind: 'Knob',
  icon: GaugeIcon,
  labelKey: 'knob',
  Component: Knob,
  Properties: KnobProperties,
};
