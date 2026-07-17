//! 将 appearance 设置应用到 CSS 变量
//!
//! 修改 :root 上的 --font-ui / --font-mono / --font-size-ui / --font-size-mono
//! 同时根据 theme 切换 dark / light 主题 (目前仅 dark, light 占位)

import type { AppSettings } from '../settings/defaults';

export function applyAppearance(appearance: AppSettings['appearance']): void {
  const root = document.documentElement;
  root.style.setProperty('--font-ui', appearance.uiFontFamily);
  root.style.setProperty('--font-mono', appearance.monoFontFamily);
  root.style.setProperty('--font-size-ui', `${appearance.uiFontSize}px`);
  root.style.setProperty('--font-size-mono', `${appearance.monoFontSize}px`);
  // body 元素的 font-size 直接控制 UI 字号
  document.body.style.fontSize = `${appearance.uiFontSize}px`;
  document.body.style.fontFamily = appearance.uiFontFamily;
  // 主题切换 (data-theme 用于 CSS 选择器, 未来扩展 light)
  root.dataset['theme'] = appearance.theme;
  // 控件栏/状态栏可见性由 App.tsx 读取 settings 控制, 这里不直接操作
}
