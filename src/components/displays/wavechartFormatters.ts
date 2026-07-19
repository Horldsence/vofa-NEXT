/// 波形图格式化工具函数

/// 读取当前主题波形图颜色 (回退到常量)
export function getThemeColor(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

/// 格式化时间 (毫秒) — 示波器风格: 自动选择 µs/ms/s 单位, 负数表示过去时间
/// 例: -250 → "-250ms", 1.5 → "1.500ms", 1500 → "1.500s", 0.1 → "100µs"
export function formatTimeMs(ms: number): string {
  const sign = ms < 0 ? '-' : '';
  const abs = Math.abs(ms);
  if (abs >= 1000) return sign + (abs / 1000).toFixed(3) + 's';
  if (abs >= 1) return sign + abs.toFixed(abs < 10 ? 3 : abs < 100 ? 2 : 1) + 'ms';
  return sign + (abs * 1000).toFixed(0) + 'µs';
}

/// 格式化 Y 轴值 — 不使用 µ/m/k 前缀, 大/小值用科学计数法, 中间值用普通小数
/// unit 为空字符串时不附加单位 (示波器默认配置)
/// 例: (1.234, 'V') → "1.234V", (0.0001234, 'A') → "1.23e-4A", (12345, '') → "1.23e+4"
export function formatYValue(val: number, unit: string): string {
  const u = unit || '';
  const abs = Math.abs(val);
  if (abs === 0) return '0' + u;
  // 大值 (>=1e4) 或小值 (<1e-3) 用科学计数法
  if (abs >= 1e4 || abs < 1e-3) return val.toExponential(2) + u;
  // 中间值用普通小数, 自适应位数
  if (abs >= 100) return val.toFixed(2) + u;
  if (abs >= 1) return val.toFixed(3) + u;
  return val.toFixed(4) + u;
}
