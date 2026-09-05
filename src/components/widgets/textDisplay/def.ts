// ============ 文本展示控件定义 ============

import { FileText as TextDisplayIcon } from 'lucide-react';
import { TextDisplay } from './TextDisplayWidget';
import type { WidgetDef } from '../registryTypes';

/// 文本展示控件定义 (配置项暂无专属编辑节)
export const textDisplayDef: WidgetDef<'TextDisplay'> = {
  kind: 'TextDisplay',
  icon: TextDisplayIcon,
  labelKey: 'textDisplay',
  Component: TextDisplay,
};
