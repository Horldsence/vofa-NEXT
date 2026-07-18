/// 波形图共享常量与工具函数

/// 通道颜色 (与 AxisSettings 的 CHANNEL_TAB_COLORS 保持一致)
export const CHANNEL_COLORS = [
  '#75beff', '#89d185', '#e2c08d', '#f48771',
  '#c586c0', '#4ec9b0', '#dcdcaa', '#9cdcfe',
];

/// 派生 series (Math/Filter 等节点输出) 颜色 — 橙色, 与通道色区分
export const DERIVED_COLORS = [
  '#ff8c42', '#ff5e5e', '#b266ff', '#00d9ff',
  '#ffd700', '#ff66b2', '#66ff66', '#ffffff',
];

export const TEXT_COLOR = '#bbbbbb';
export const GRID_COLOR = '#444444';
export const TICK_COLOR = '#555555';
export const CURSOR_COLOR = '#ffd700';

/// 时间轴缩略图内边距
export const TIMELINE_PAD = 4;

/// 获取容器尺寸 (下限保护)
export function getContainerSize(container: HTMLElement) {
  const rect = container.getBoundingClientRect();
  return {
    w: Math.max(Math.floor(rect.width), 300),
    h: Math.max(Math.floor(rect.height), 200),
  };
}
