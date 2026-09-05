// ============ 逆 FFT 求解器定义 ============

import { Activity } from 'lucide-react';
import { IFFTWidget } from './IFFTWidget';
import type { WidgetDef } from '../registryTypes';

/// 逆 FFT 求解器定义 — 频域源由连线解析, 无额外属性
export const ifftDef: WidgetDef<'IFFT'> = {
  kind: 'IFFT',
  icon: Activity,
  labelKey: 'ifft',
  Component: IFFTWidget,
};
