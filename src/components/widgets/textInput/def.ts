// ============ 文本输入控件定义 ============

import { TextCursorInput as TextInputIcon } from 'lucide-react';
import { TextInput } from './TextInputWidget';
import { TextInputProperties } from './TextInputProperties';
import type { WidgetDef } from '../registryTypes';

/// 文本输入控件定义
export const textInputDef: WidgetDef<'TextInput'> = {
  kind: 'TextInput',
  icon: TextInputIcon,
  labelKey: 'textInput',
  Component: TextInput,
  Properties: TextInputProperties,
};
