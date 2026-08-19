import { describe, it, expect } from 'vitest';
import { LIGHT_THEME } from '../theme';

/// 结构性背景 token — 亮色主题下必须为浅色 (窗口底层/活动栏/侧边栏等大面积背景)
/// 彩色功能背景 (bgButton/bgDanger 等) 不在此列
const STRUCTURAL_BG_TOKENS = [
  'bgActivity',
  'bgSidebar',
  'bgEditor',
  'bgWindow',
  'bgPanelHeader',
  'bgInput',
  'bgStatusbar',
  'bgTooltip',
] as const;

/// WCAG 相对亮度 (0 = 黑, 1 = 白)
function relativeLuminance(css: string): number {
  const m = /^#([0-9a-f]{6})$/i.exec(css.trim());
  if (!m) throw new Error(`非 hex 颜色: ${css}`);
  const hex = m[1];
  const channel = (i: number) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4);
}

describe('LIGHT_THEME', () => {
  it.each(STRUCTURAL_BG_TOKENS)('结构性背景 token %s 应为浅色 (亮度 > 0.5)', (token) => {
    const value = LIGHT_THEME.tokens[token];
    expect(relativeLuminance(value), `${token} = ${value}`).toBeGreaterThan(0.5);
  });
});
