// ============ FFT 频域求解器定义 ============

import { Activity } from 'lucide-react';
import { FFTWidget } from './FFTWidget';
import { FFTProperties } from './FFTProperties';
import type { WidgetDef } from '../registryTypes';

/// FFT 频域求解器定义
export const fftDef: WidgetDef<'FFT'> = {
  kind: 'FFT',
  icon: Activity,
  labelKey: 'fft',
  Component: FFTWidget,
  Properties: FFTProperties,
};
