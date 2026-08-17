//! CSS样式主题加载器
//!
//! 管理CSS主题的加载/卸载，支持内置主题和外部URL

import type { CssStyleTheme } from './theme';

/// 已加载的CSS主题映射
const LOADED_CSS_THEMES = new Map<string, HTMLLinkElement>();

/// 加载CSS主题
export function loadCssTheme(theme: CssStyleTheme): void {
  // 卸载已加载的CSS主题
  unloadAllCssThemes();

  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = theme.url;
  link.id = `css-theme-${theme.id}`;
  document.head.appendChild(link);
  LOADED_CSS_THEMES.set(theme.id, link);
}

/// 卸载指定CSS主题
export function unloadCssTheme(themeId: string): void {
  const link = LOADED_CSS_THEMES.get(themeId);
  if (link) {
    link.remove();
    LOADED_CSS_THEMES.delete(themeId);
  }
}

/// 卸载所有已加载的CSS主题
export function unloadAllCssThemes(): void {
  for (const link of LOADED_CSS_THEMES.values()) {
    link.remove();
  }
  LOADED_CSS_THEMES.clear();
}

/// 获取当前激活的CSS主题ID
export function getActiveCssThemeId(): string | null {
  return LOADED_CSS_THEMES.keys().next().value ?? null;
}

/// 检查CSS主题是否已加载
export function isCssThemeLoaded(themeId: string): boolean {
  return LOADED_CSS_THEMES.has(themeId);
}
