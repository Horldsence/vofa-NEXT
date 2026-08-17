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
  'bgWindow',
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

/// CSS样式主题定义 (独立于颜色token的完整CSS样式包)
export interface CssStyleTheme {
  id: string;
  name: string;
  /** CSS文件URL，内置主题使用相对路径如 '/themes/default.css' */
  url: string;
  /** 是否为内置主题 */
  isBuiltIn: boolean;
}

/// 内置CSS样式主题
export const BUILT_IN_CSS_THEMES: CssStyleTheme[] = [
  {
    id: 'default',
    name: 'Default',
    url: '/themes/default.css',
    isBuiltIn: true,
  },
  {
    id: 'monet',
    name: 'Monet',
    url: '/themes/monet.css',
    isBuiltIn: true,
  },
];

export function isBuiltInCssThemeId(id: string): boolean {
  return BUILT_IN_CSS_THEMES.some((t) => t.id === id);
}

export function getBuiltInCssTheme(id: string): CssStyleTheme | undefined {
  return BUILT_IN_CSS_THEMES.find((t) => t.id === id);
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
  bgWindow: 'background',
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
  bgWindow: '窗口背景',
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

/// 语义 token 别名: 语义 CSS 变量名 -> 其对应的原始 token。
/// 与 index.css @theme 中的语义层保持一致; 纯增量, 不改动 THEME_TOKENS 可编辑集合。
export const SEMANTIC_TOKEN_ALIASES: Readonly<Record<string, ThemeToken>> = {
  '--color-bg-surface': 'bgEditor',
  '--color-bg-elevated': 'bgSidebar',
  '--color-bg-inset': 'bgInput',
  '--color-text-muted': 'textDisabled',
  '--color-border-subtle': 'border',
  '--color-accent-hover': 'bgButtonHover',
  '--color-accent-active': 'bgActive',
  '--color-danger': 'red',
  '--color-warning': 'yellow',
  '--color-success': 'green',
  '--color-info': 'blue',
  '--color-danger-surface': 'bgDanger',
};

/// 暗色主题 — 当前项目默认 (现代冷调深色)
export const DARK_THEME: ThemeDefinition = {
  id: 'dark',
  name: 'Dark',
  isBuiltIn: true,
  tokens: {
    bgActivity: '#1c2028',
    bgSidebar: '#1b1f26',
    bgEditor: '#14171f',
    bgWindow: '#0d1016',
    bgPanelHeader: '#21262f',
    bgInput: '#262b36',
    bgHover: '#272c37',
    bgActive: '#34435c',
    bgButton: '#3b6fd0',
    bgButtonHover: '#4a7ee0',
    bgStatusbar: '#1a1e25',
    bgDanger: '#5a2626',
    bgDangerHover: '#6e2d2d',
    bgTooltip: '#21262f',
    bgScrollbar: '#333a46',
    bgScrollbarHover: '#434b5a',
    bgNodeHeader: 'rgba(255, 255, 255, 0.06)',
    border: '#2e333e',
    borderNodeHeader: 'rgba(255, 255, 255, 0.10)',
    textPrimary: '#e2e6ed',
    textSecondary: '#9aa1ad',
    textBright: '#ffffff',
    textDisabled: '#666d79',
    textInverse: '#ffffff',
    accent: '#4c8dff',
    green: '#89d185',
    red: '#f48771',
    yellow: '#e2c08d',
    blue: '#6cb2ff',
    purple: '#c586c0',
    orange: '#ce9178',
    waveformGrid: '#303540',
    waveformText: '#b4bac6',
    waveformTick: '#4a505c',
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
    bgWindow: '#f0f0f0',
    bgPanelHeader: '#e8e8e8',
    bgInput: '#ffffff',
    bgHover: '#e8e8e8',
    bgActive: '#e3f2fd',
    bgButton: '#007acc',
    bgButtonHover: '#005f9e',
    bgStatusbar: '#f0f0f0',
    bgDanger: '#ffeaea',
    bgDangerHover: '#ffd6d6',
    bgTooltip: '#f3f3f3',
    bgScrollbar: '#c4c4c4',
    bgScrollbarHover: '#a0a0a0',
    bgNodeHeader: 'rgba(0, 0, 0, 0.05)',
    border: '#e5e5e5',
    borderNodeHeader: 'rgba(0, 0, 0, 0.10)',
    textPrimary: '#0f0f0f',
    textSecondary: '#4d535a',
    textBright: '#000000',
    textDisabled: '#888d94',
    textInverse: '#ffffff',
    accent: '#007acc',
    green: '#388a34',
    red: '#cd3131',
    yellow: '#bc5a00',
    blue: '#007acc',
    purple: '#af00db',
    orange: '#aa5d00',
    waveformGrid: '#d4d4d4',
    waveformText: '#555b62',
    waveformTick: '#a0a0a0',
    waveformCursor: '#b8860b',
  },
};

/// 莫奈主题 — 低饱和灰调深色 (雾霾蓝/灰绿/灰紫/陶土, 配合分层卡片设计)
export const MONET_THEME: ThemeDefinition = {
  id: 'monet',
  name: 'Monet',
  isBuiltIn: true,
  tokens: {
    bgActivity: '#21262c',
    bgSidebar: '#21262b',
    bgEditor: '#191d22',
    bgWindow: '#111519',
    bgPanelHeader: '#252b31',
    bgInput: '#2a3037',
    bgHover: '#2b3138',
    bgActive: '#3d4a5c',
    bgButton: '#5b7fa6',
    bgButtonHover: '#6b8fb6',
    bgStatusbar: '#1d2126',
    bgDanger: '#59302c',
    bgDangerHover: '#6a3833',
    bgTooltip: '#252b31',
    bgScrollbar: '#353b44',
    bgScrollbarHover: '#434a55',
    bgNodeHeader: 'rgba(255, 255, 255, 0.06)',
    border: '#343a42',
    borderNodeHeader: 'rgba(255, 255, 255, 0.10)',
    textPrimary: '#dfe3e7',
    textSecondary: '#9aa2ab',
    textBright: '#ffffff',
    textDisabled: '#666e77',
    textInverse: '#ffffff',
    accent: '#7aa2c9',
    green: '#8fa98a',
    red: '#c97e6f',
    yellow: '#c9b083',
    blue: '#82a8cc',
    purple: '#a58fb5',
    orange: '#c08b6d',
    waveformGrid: '#2e343d',
    waveformText: '#b2b9c2',
    waveformTick: '#4a515b',
    waveformCursor: '#d9c27a',
  },
};

/// 内置主题列表
export const BUILT_IN_THEMES: ThemeDefinition[] = [DARK_THEME, LIGHT_THEME, MONET_THEME];

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

/// 将语义变量以别名形式写入 :root。
/// 值保持 var(--color-*) 形式, 运行时动态解析到原始 token —— 自定义主题
/// 与亚克力透明化 (applyAppearance 对原始背景 token 的 rgba 变换) 仍能穿透生效。
export function applySemanticTheme(): void {
  const root = document.documentElement;
  for (const [semVar, rawToken] of Object.entries(SEMANTIC_TOKEN_ALIASES)) {
    root.style.setProperty(semVar, `var(${getCssVariableName(rawToken)})`);
  }
}

/// 将主题应用到 DOM
export function applyTheme(theme: ThemeDefinition): void {
  const root = document.documentElement;
  for (const token of THEME_TOKENS) {
    root.style.setProperty(getCssVariableName(token), theme.tokens[token]);
  }
  applySemanticTheme();
  root.dataset.theme = theme.isBuiltIn ? theme.id : `custom-${theme.id}`;
}

/// 更新单个 token (用于编辑器实时预览)
export function applyThemeToken(token: ThemeToken, value: string): void {
  document.documentElement.style.setProperty(getCssVariableName(token), value);
}
