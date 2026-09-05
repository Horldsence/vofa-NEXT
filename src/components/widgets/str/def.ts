// ============ 字符串操作控件定义 ============

import { Type as StrIcon } from 'lucide-react';
import { StrWidget } from './StrWidget';
import { StrProperties } from './StrProperties';
import type { WidgetDef } from '../registryTypes';

/// 字符串操作控件定义
export const strDef: WidgetDef<'Str'> = {
  kind: 'Str',
  icon: StrIcon,
  labelKey: 'str',
  Component: StrWidget,
  Properties: StrProperties,
};
