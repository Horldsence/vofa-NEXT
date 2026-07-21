//! 将 appearance 设置应用到 CSS 变量
//!
//! 修改 :root 上的字体变量与颜色 token, 并激活对应主题。
//! 亚克力背景开启时, 在主题应用后把背景类 token 转为半透明 rgba,
//! 并通知原生窗口启用系统毛玻璃效果。

import { invoke } from '@tauri-apps/api/core';
import type { AppSettings } from '../settings/defaults';
import { applyTheme, getCssVariableName, resolveActiveTheme, type ThemeToken } from './theme';

/// 亚克力模式下各背景 token 的透明度 (未列出的 token 保持不透明)
const ACRYLIC_TOKEN_ALPHA: Partial<Record<ThemeToken, number>> = {
  bgActivity: 0.6,
  bgSidebar: 0.65,
  bgEditor: 0.55,
  bgPanelHeader: 0.65,
  bgInput: 0.6,
  bgHover: 0.6,
};

/// 将 #rgb / #rrggbb 颜色转为带透明度的 rgba; 非 hex 输入原样返回
function withAlpha(color: string, alpha: number): string {
  const m = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.exec(color.trim());
  if (!m) return color;
  let hex = m[1];
  if (hex.length === 3) {
    hex = hex.split('').map((c) => c + c).join('');
  }
  const r = parseInt(hex.slice(0, 2), 16);
  const g = parseInt(hex.slice(2, 4), 16);
  const b = parseInt(hex.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

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
  // 亚克力背景: 背景 token 半透明化 + 原生窗口毛玻璃
  const acrylic = appearance.acrylicBackground === true;
  if (acrylic) {
    root.dataset.acrylic = 'true';
    for (const [token, alpha] of Object.entries(ACRYLIC_TOKEN_ALPHA) as [ThemeToken, number][]) {
      const varName = getCssVariableName(token);
      const value = root.style.getPropertyValue(varName);
      root.style.setProperty(varName, withAlpha(value, alpha));
    }
  } else {
    delete root.dataset.acrylic;
  }
  // 纯浏览器 dev 环境无 Tauri 后端, 调用失败时静默忽略
  invoke('set_window_acrylic', { enabled: acrylic }).catch(() => {});
  // 控件栏/状态栏可见性由 App.tsx 读取 settings 控制, 这里不直接操作
}
