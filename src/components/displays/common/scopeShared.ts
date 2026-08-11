import type { ReactNode } from 'react';

/// 通用档位下拉选择函数签名
export type RenderStepSelect = (
  steps: number[],
  value: number,
  onPick: (v: number) => void,
  format: (v: number) => string
) => ReactNode;

/// Tab 通道颜色指示点 (与 WaveformChart 的 CHANNEL_COLORS 保持一致)
export const CHANNEL_TAB_COLORS = [
  '#75beff', '#89d185', '#e2c08d', '#f48771',
  '#c586c0', '#4ec9b0', '#dcdcaa', '#9cdcfe',
];
