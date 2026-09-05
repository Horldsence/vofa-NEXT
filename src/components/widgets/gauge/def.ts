import { Gauge as GaugeIcon } from 'lucide-react';
import { Gauge } from './GaugeWidget';
import { GaugeProperties } from './GaugeProperties';
import type { WidgetDef } from '../registryTypes';

/// 仪表盘控件定义
export const gaugeDef: WidgetDef<'Gauge'> = {
  kind: 'Gauge',
  icon: GaugeIcon,
  labelKey: 'gauge',
  Component: Gauge,
  Properties: GaugeProperties,
};
