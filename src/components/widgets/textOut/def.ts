// ============ 文本下发控件定义 ============

import { Send as TextOutIcon } from 'lucide-react';
import { TextOutWidget } from './TextOutWidget';
import { TextOutProperties } from './TextOutProperties';
import type { WidgetDef } from '../registryTypes';

/// 文本下发控件定义
export const textOutDef: WidgetDef<'TextOut'> = {
  kind: 'TextOut',
  icon: TextOutIcon,
  labelKey: 'textOut',
  Component: TextOutWidget,
  Properties: TextOutProperties,
};
