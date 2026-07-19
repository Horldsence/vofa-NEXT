//! 主题系统 — 内置/自定义主题定义与应用
//!
//! 所有颜色通过 CSS 变量注入, Tailwind v4 的 @theme 工具类会自动读取这些变量。
//! applyTheme() 直接修改 :root 上的 CSS 变量, 因此内置 light 主题和自定义主题
//! 走同一条路径, 无需额外 CSS 覆盖块。

import type { AppSettings } from './defaults';

/// 可定制的颜色 token (camelCase) -> CSS 变量名 (--color-xxx)
export const THEME_TOKENS = [
  // 背景
  'bgActivity',
  'bgSidebar',
  'bgEditor',
  'bgPanelHeader',
  'bgInput',
  'bgHover',
  'bgActive',
  'bgButton',
  'bgButtonHover',
  'bgStatusbar',
  'bgDanger',
  'bgDangerHover',
  'bgTooltip',
  'bgScrollbar',
  'bgScrollbarHover',
  'bgNodeHeader',
  // 边框
  'border',
  'borderNodeHeader',
  // 文字
  'textPrimary',
  'textSecondary',
  'textBright',
  'textDisabled',
  'textInverse',
  // 强调/语义色
  'accent',
  'green',
  'red',
  'yellow',
  'blue',
  'purple',
  'orange',
  // 波形图专用
  'waveformGrid',
  'waveformText',
  'waveformTick',
  'waveformCursor',
] as const;

export type ThemeToken = (typeof THEME_TOKENS)[number];

/// 主题定义
export interface ThemeDefinition {
  id: string;
  name: string;
  isBuiltIn: boolean;
  tokens: Record<ThemeToken, string>;
}

/// token 分组 (用于编辑器归类)
export const TOKEN_GROUPS = {
  background: 'background',
  border: 'border',
  text: 'text',
  accent: 'accent',
} as const;

export type TokenGroup = (typeof TOKEN_GROUPS)[keyof typeof TOKEN_GROUPS];

const TOKEN_GROUP_MAP: Record<ThemeToken, TokenGroup> = {
  bgActivity: 'background',
  bgSidebar: 'background',
  bgEditor: 'background',
  bgPanelHeader: 'background',
  bgInput: 'background',
  bgHover: 'background',
  bgActive: 'background',
  bgButton: 'background',
  bgButtonHover: 'background',
  bgStatusbar: 'background',
  bgDanger: 'background',
  bgDangerHover: 'background',
  bgTooltip: 'background',
  bgScrollbar: 'background',
  bgScrollbarHover: 'background',
  bgNodeHeader: 'background',
  border: 'border',
  borderNodeHeader: 'border',
  textPrimary: 'text',
  textSecondary: 'text',
  textBright: 'text',
  textDisabled: 'text',
  textInverse: 'text',
  accent: 'accent',
  green: 'accent',
  red: 'accent',
  yellow: 'accent',
  blue: 'accent',
  purple: 'accent',
  orange: 'accent',
  waveformGrid: 'accent',
  waveformText: 'accent',
  waveformTick: 'accent',
  waveformCursor: 'accent',
};

export function getTokenGroup(token: ThemeToken): TokenGroup {
  return TOKEN_GROUP_MAP[token];
}

/// token 显示标签 (中文), 仅用于主题编辑器
export const TOKEN_LABELS: Record<ThemeToken, string> = {
  bgActivity: '活动栏背景',
  bgSidebar: '侧边栏背景',
  bgEditor: '编辑器背景',
  bgPanelHeader: '面板标题背景',
  bgInput: '输入框背景',
  bgHover: '悬停背景',
  bgActive: '激活背景',
  bgButton: '按钮背景',
  bgButtonHover: '按钮悬停背景',
  bgStatusbar: '状态栏背景',
  bgDanger: '危险背景',
  bgDangerHover: '危险悬停背景',
  bgTooltip: '工具提示背景',
  bgScrollbar: '滚动条背景',
  bgScrollbarHover: '滚动条悬停背景',
  bgNodeHeader: '节点标题背景',
  border: '边框',
  borderNodeHeader: '节点标题边框',
  textPrimary: '主要文字',
  textSecondary: '次要文字',
  textBright: '高亮文字',
  textDisabled: '禁用文字',
  textInverse: '反色文字(用于彩色背景)',
  accent: '强调色',
  green: '绿色',
  red: '红色',
  yellow: '黄色',
  blue: '蓝色',
  purple: '紫色',
  orange: '橙色',
  waveformGrid: '波形图网格',
  waveformText: '波形图文字',
  waveformTick: '波形图刻度',
  waveformCursor: '波形图光标',
};

/// token -> CSS 变量名
export function getCssVariableName(token: ThemeToken): string {
  // camelCase -> kebab-case, 加 --color- 前缀
  const kebab = token.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
  return `--color-${kebab}`;
}

/// 暗色主题 — 当前项目默认
export const DARK_THEME: ThemeDefinition = {
  id: 'dark',
  name: 'Dark',
  isBuiltIn: true,
  tokens: {
    bgActivity: '#333333',
    bgSidebar: '#252526',
    bgEditor: '#1e1e1e',
    bgPanelHeader: '#2d2d2d',
    bgInput: '#3c3c3c',
    bgHover: '#2a2d2e',
    bgActive: '#094771',
    bgButton: '#0e639c',
    bgButtonHover: '#1177bb',
    bgStatusbar: '#007acc',
    bgDanger: '#5a1d1d',
    bgDangerHover: '#6b2424',
    bgTooltip: '#252526',
    bgScrollbar: '#424242',
    bgScrollbarHover: '#4f4f4f',
    bgNodeHeader: 'rgba(255, 255, 255, 0.06)',
    border: '#3c3c3c',
    borderNodeHeader: 'rgba(255, 255, 255, 0.1)',
    textPrimary: '#cccccc',
    textSecondary: '#858585',
    textBright: '#ffffff',
    textDisabled: '#666666',
    textInverse: '#ffffff',
    accent: '#007acc',
    green: '#89d185',
    red: '#f48771',
    yellow: '#e2c08d',
    blue: '#75beff',
    purple: '#c586c0',
    orange: '#ce9178',
    waveformGrid: '#444444',
    waveformText: '#bbbbbb',
    waveformTick: '#555555',
    waveformCursor: '#ffd700',
  },
};

/// 浅色主题 — VSCode Light 风格
export const LIGHT_THEME: ThemeDefinition = {
  id: 'light',
  name: 'Light',
  isBuiltIn: true,
  tokens: {
    bgActivity: '#2c2c2c',
    bgSidebar: '#f3f3f3',
    bgEditor: '#ffffff',
    bgPanelHeader: '#e8e8e8',
    bgInput: '#ffffff',
    bgHover: '#e8e8e8',
    bgActive: '#e3f2fd',
    bgButton: '#007acc',
    bgButtonHover: '#005f9e',
    bgStatusbar: '#007acc',
    bgDanger: '#ffeaea',
    bgDangerHover: '#ffd6d6',
    bgTooltip: '#f3f3f3',
    bgScrollbar: '#c4c4c4',
    bgScrollbarHover: '#a0a0a0',
    bgNodeHeader: 'rgba(0, 0, 0, 0.04)',
    border: '#e5e5e5',
    borderNodeHeader: 'rgba(0, 0, 0, 0.08)',
    textPrimary: '#1e1e1e',
    textSecondary: '#6e6e6e',
    textBright: '#1e1e1e',
    textDisabled: '#a6a6a6',
    textInverse: '#ffffff',
    accent: '#007acc',
    green: '#388a34',
    red: '#cd3131',
    yellow: '#bc5a00',
    blue: '#007acc',
    purple: '#af00db',
    orange: '#aa5d00',
    waveformGrid: '#d4d4d4',
    waveformText: '#6e6e6e',
    waveformTick: '#a0a0a0',
    waveformCursor: '#b8860b',
  },
};

/// 内置主题列表
export const BUILT_IN_THEMES: ThemeDefinition[] = [DARK_THEME, LIGHT_THEME];

export function isBuiltInThemeId(id: string): boolean {
  return BUILT_IN_THEMES.some((t) => t.id === id);
}

export function getBuiltInTheme(id: string): ThemeDefinition | undefined {
  return BUILT_IN_THEMES.find((t) => t.id === id);
}

/// 创建空自定义主题 (基于指定基础主题)
export function createCustomTheme(
  name: string,
  baseTheme: ThemeDefinition = DARK_THEME,
  id?: string
): ThemeDefinition {
  return {
    id: id ?? `custom-${Date.now()}`,
    name,
    isBuiltIn: false,
    tokens: { ...baseTheme.tokens },
  };
}

/// 从设置中解析当前激活主题
export function resolveActiveTheme(appearance: AppSettings['appearance']): ThemeDefinition {
  const builtIn = getBuiltInTheme(appearance.theme);
  if (builtIn) return builtIn;
  const custom = appearance.customThemes?.find((t) => t.id === appearance.theme);
  if (custom) return custom;
  return DARK_THEME;
}

/// 将主题应用到 DOM
export function applyTheme(theme: ThemeDefinition): void {
  const root = document.documentElement;
  for (const token of THEME_TOKENS) {
    root.style.setProperty(getCssVariableName(token), theme.tokens[token]);
  }
  root.dataset.theme = theme.isBuiltIn ? theme.id : `custom-${theme.id}`;
}

/// 更新单个 token (用于编辑器实时预览)
export function applyThemeToken(token: ThemeToken, value: string): void {
  document.documentElement.style.setProperty(getCssVariableName(token), value);
}
