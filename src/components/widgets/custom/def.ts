// ============ 自定义 JS 控件定义 ============

import { Code2 as CodeIcon } from 'lucide-react';
import { CustomWidget } from './CustomWidget';
import { CustomProperties } from './CustomProperties';
import type { WidgetDef } from '../registryTypes';

/// 自定义 JS 控件定义
export const customDef: WidgetDef<'Custom'> = {
  kind: 'Custom',
  icon: CodeIcon,
  labelKey: 'custom',
  Component: CustomWidget,
  Properties: CustomProperties,
};
