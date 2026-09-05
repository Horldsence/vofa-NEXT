// ============ 图像控件定义 ============

import { Image as ImageIcon } from 'lucide-react';
import { ImageViewer } from './ImageWidget';
import { ImageProperties } from './ImageProperties';
import type { WidgetDef } from '../registryTypes';

/// 图像控件定义
export const imageDef: WidgetDef<'Image'> = {
  kind: 'Image',
  icon: ImageIcon,
  labelKey: 'image',
  Component: ImageViewer,
  Properties: ImageProperties,
};
