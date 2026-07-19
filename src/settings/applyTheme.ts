//! 将 appearance 设置应用到 CSS 变量
//!
//! 修改 :root 上的字体变量与颜色 token, 并激活对应主题。

import type { AppSettings } from '../settings/defaults';
import { applyTheme, resolveActiveTheme } from './theme';

export function applyAppearance(appearance: AppSettings['appearance']): void {
  const root = document.documentElement;
  root.style.setProperty('--font-ui', appearance.uiFontFamily);
  root.style.setProperty('--font-mono', appearance.monoFontFamily);
  root.style.setProperty('--font-size-ui', `${appearance.uiFontSize}px`);
  root.style.setProperty('--font-size-mono', `${appearance.monoFontSize}px`);
  // body 元素的 font-size 直接控制 UI 字号
  document.body.style.fontSize = `${appearance.uiFontSize}px`;
  document.body.style.fontFamily = appearance.uiFontFamily;
  // 应用主题颜色
  applyTheme(resolveActiveTheme(appearance));
  // 控件栏/状态栏可见性由 App.tsx 读取 settings 控制, 这里不直接操作
}
