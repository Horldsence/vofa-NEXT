//! 统一CSS变量常量导出
//!
//! 供组件和CSS插件使用，确保所有CSS变量名在单一位置定义

/// 背景色CSS变量
export const BG_VARS = {
  activity: '--color-bg-activity',
  sidebar: '--color-bg-sidebar',
  editor: '--color-bg-editor',
  window: '--color-bg-window',
  panelHeader: '--color-bg-panel-header',
  input: '--color-bg-input',
  hover: '--color-bg-hover',
  active: '--color-bg-active',
  button: '--color-bg-button',
  buttonHover: '--color-bg-button-hover',
  statusbar: '--color-bg-statusbar',
  danger: '--color-bg-danger',
  dangerHover: '--color-bg-danger-hover',
  tooltip: '--color-bg-tooltip',
  scrollbar: '--color-bg-scrollbar',
  scrollbarHover: '--color-bg-scrollbar-hover',
  nodeHeader: '--color-bg-node-header',
  overlay: '--color-bg-overlay',
  subtle: '--color-bg-subtle',
  scrim: '--color-bg-scrim',
} as const;

/// 边框色CSS变量
export const BORDER_VARS = {
  default: '--color-border',
  nodeHeader: '--color-border-node-header',
} as const;

/// 文字色CSS变量
export const TEXT_VARS = {
  primary: '--color-text-primary',
  secondary: '--color-text-secondary',
  bright: '--color-text-bright',
  disabled: '--color-text-disabled',
  inverse: '--color-text-inverse',
} as const;

/// 强调/语义色CSS变量
export const ACCENT_VARS = {
  default: '--color-accent',
  hover: '--color-accent-hover',
  active: '--color-accent-active',
  success: '--color-success',
  warning: '--color-warning',
  danger: '--color-danger',
  info: '--color-info',
  green: '--color-green',
  red: '--color-red',
  yellow: '--color-yellow',
  blue: '--color-blue',
  purple: '--color-purple',
  orange: '--color-orange',
} as const;

/// 语义层CSS变量 (映射到原始token)
export const SEMANTIC_VARS = {
  surface: '--color-bg-surface',
  elevated: '--color-bg-elevated',
  inset: '--color-bg-inset',
  muted: '--color-text-muted',
  borderSubtle: '--color-border-subtle',
  dangerSurface: '--color-danger-surface',
} as const;

/// 波形图专用CSS变量
export const WAVEFORM_VARS = {
  grid: '--color-waveform-grid',
  text: '--color-waveform-text',
  tick: '--color-waveform-tick',
  cursor: '--color-waveform-cursor',
} as const;

/// 字体CSS变量
export const FONT_VARS = {
  ui: '--font-ui',
  mono: '--font-mono',
} as const;

/// 字号CSS变量
export const FONT_SIZE_VARS = {
  xs: '--font-size-xs',
  sm: '--font-size-sm',
  base: '--font-size-base',
  lg: '--font-size-lg',
  xl: '--font-size-xl',
  '2xl': '--font-size-2xl',
  '3xl': '--font-size-3xl',
} as const;

/// 圆角CSS变量
export const RADIUS_VARS = {
  default: '--radius-default',
  sm: '--radius-sm',
  md: '--radius-md',
  lg: '--radius-lg',
  xl: '--radius-xl',
  '2xl': '--radius-2xl',
} as const;

/// 间距CSS变量
export const SPACING_VARS = {
  1: '--spacing-1',
  2: '--spacing-2',
  3: '--spacing-3',
  4: '--spacing-4',
  5: '--spacing-5',
  6: '--spacing-6',
  8: '--spacing-8',
} as const;

/// z-index CSS变量
export const Z_INDEX_VARS = {
  dropdown: '--z-dropdown',
  tooltip: '--z-tooltip',
  contextMenu: '--z-context-menu',
  modal: '--z-modal',
  toast: '--z-toast',
} as const;

/// 阴影CSS变量
export const SHADOW_VARS = {
  modal: '--shadow-modal',
} as const;

/// 动效CSS变量
export const TRANSITION_VARS = {
  fast: '--transition-fast',
  base: '--transition-base',
  slow: '--transition-slow',
  'duration-fast': '--transition-duration-fast',
  'duration-base': '--transition-duration-base',
  'duration-slow': '--transition-duration-slow',
} as const;

/// 缓动函数CSS变量
export const EASE_VARS = {
  'out-cubic': '--ease-out-cubic',
  'in-out': '--ease-in-out',
} as const;

/// 统一CSS变量命名空间
export const CSS_VARS = {
  bg: BG_VARS,
  border: BORDER_VARS,
  text: TEXT_VARS,
  accent: ACCENT_VARS,
  semantic: SEMANTIC_VARS,
  waveform: WAVEFORM_VARS,
  font: FONT_VARS,
  fontSize: FONT_SIZE_VARS,
  radius: RADIUS_VARS,
  spacing: SPACING_VARS,
  zIndex: Z_INDEX_VARS,
  shadow: SHADOW_VARS,
  transition: TRANSITION_VARS,
  ease: EASE_VARS,
} as const;

export type CssVariableName = string;
