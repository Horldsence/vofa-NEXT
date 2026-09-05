// ============ 标签控件定义 ============

import { Tag as TagIcon } from 'lucide-react';
import { Label } from './LabelWidget';
import { LabelProperties } from './LabelProperties';
import type { WidgetDef } from '../registryTypes';

/// 标签控件定义
export const labelDef: WidgetDef<'Label'> = {
  kind: 'Label',
  icon: TagIcon,
  labelKey: 'label',
  Component: Label,
  Properties: LabelProperties,
};
