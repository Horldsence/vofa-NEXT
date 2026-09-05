import { Sliders } from 'lucide-react';
import { Slider } from './SliderWidget';
import { SliderProperties } from './SliderProperties';
import type { WidgetDef } from '../registryTypes';

/// 滑块控件定义
export const sliderDef: WidgetDef<'Slider'> = {
  kind: 'Slider',
  icon: Sliders,
  labelKey: 'slider',
  Component: Slider,
  Properties: SliderProperties,
};
