// ============ 单选控件定义 ============

import { Radio as RadioIcon } from 'lucide-react';
import { Radio } from './RadioWidget';
import { RadioProperties } from './RadioProperties';
import type { WidgetDef } from '../registryTypes';

/// 单选控件定义
export const radioDef: WidgetDef<'Radio'> = {
  kind: 'Radio',
  icon: RadioIcon,
  labelKey: 'radio',
  Component: Radio,
  Properties: RadioProperties,
};
