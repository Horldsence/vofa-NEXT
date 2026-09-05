// ============ 显示控件量程/刻度 ============
//
// Gauge / Progress 等显示控件共用的量程与刻度纯函数:
// - 手动模式: 直接使用用户配置的 min/max, 刻度均分;
// - 自动模式: 滑动窗口观测 min/max (见 lib/data/autoRange.ts) 后, 选 1-2-5
//   步距并把量程边界外扩到步距整数倍 — 主刻度恰好落在边界上, 标签整齐。

export interface DisplayRange {
  min: number;
  max: number;
}

/// 最小量程跨度 — 防止零跨度除零与画布退化
const MIN_SPAN = 1e-9;

/// 1-2-5 刻度步距 — 按 (跨度 / 目标格数) 向上取最近 1-2-5 档
/// (与示波器 snapVPerDivUp 同族, 面向刻度步距; 容差抵消浮点误差)
export function niceStep(span: number, targetSteps: number): number {
  if (!Number.isFinite(span) || span <= MIN_SPAN || targetSteps < 1) return 1;
  const raw = span / targetSteps;
  const decade = Math.pow(10, Math.floor(Math.log10(raw)));
  const m = raw / decade;
  const mantissa = m <= 1 + 1e-9 ? 1 : m <= 2 + 1e-9 ? 2 : m <= 5 + 1e-9 ? 5 : 10;
  return mantissa * decade;
}

function niceFloor(v: number, step: number): number {
  return Math.floor(v / step + 1e-9) * step;
}

function niceCeil(v: number, step: number): number {
  return Math.ceil(v / step - 1e-9) * step;
}

/// 由观测极值构造「好刻度」量程 — 自动模式用:
/// 步距按跨度与目标格数取整, 边界向外扩到步距整数倍;
/// 平直信号 (跨度≈0) 以该值为中心取 ±1, 避免零跨度。
export function computeNiceRange(vMin: number, vMax: number, majorTicks: number): DisplayRange {
  if (!Number.isFinite(vMin) || !Number.isFinite(vMax)) return { min: 0, max: 1 };
  if (vMax < vMin) [vMin, vMax] = [vMax, vMin];
  if (vMax - vMin <= MIN_SPAN) {
    // 平直信号: 以整数值为中心取 ±1, 避免零跨度且刻度落整数
    const c = Math.round(vMin);
    return { min: c - 1, max: c + 1 };
  }
  const step = niceStep(vMax - vMin, Math.max(1, majorTicks - 1));
  return { min: niceFloor(vMin, step), max: niceCeil(vMax, step) };
}

/// 量程内的主刻度标签值 (含两端) — 均分 majorTicks 个标签
/// 自动模式边界已是步距整数倍, 均分结果即落在步距上。
export function tickValues(range: DisplayRange, majorTicks: number): number[] {
  const ticks = Math.max(2, Math.round(majorTicks));
  const span = range.max - range.min;
  if (!Number.isFinite(span) || span <= 0) return [range.min];
  const values: number[] = [];
  for (let i = 0; i < ticks; i++) {
    values.push(range.min + (span * i) / (ticks - 1));
  }
  return values;
}

/// 步距/间距对应的小数位 (标签精度 auto 模式), 上限 6 位:
/// 找到最小的 d 使 step×10^d 为整数 (相对容差抵消浮点噪声)
export function decimalsForStep(step: number): number {
  if (!Number.isFinite(step) || step <= 0) return 0;
  for (let d = 0; d <= 6; d++) {
    const scaled = step * Math.pow(10, d);
    if (Math.abs(scaled - Math.round(scaled)) < 1e-6 * Math.max(1, Math.abs(scaled))) return d;
  }
  return 6;
}

/// 刻度/读数格式化 — precision 'auto' 由量程与刻度数推导
/// (间距即最小可分辨量, auto 精度取其数量级; 显式精度钳制 0..6)
export function formatTick(
  v: number,
  precision: 'auto' | number,
  range: DisplayRange,
  majorTicks: number,
): string {
  const spacing = (range.max - range.min) / Math.max(1, Math.round(majorTicks) - 1);
  const digits = precision === 'auto'
    ? decimalsForStep(spacing)
    : Math.max(0, Math.min(6, Math.round(precision)));
  return v.toFixed(digits);
}
