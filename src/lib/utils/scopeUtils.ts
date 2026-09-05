import { type Coupling } from '../../types';

const H_DIVS = 10;
const V_DIVS = 8;

/// 耦合方式数据变换 - 对原始通道数据应用 DC/AC/GND 耦合 (渲染用)
/// DC: 直通; AC: 减去窗口内非 NaN 均值 (去除直流分量); GND: 全部置 0 (显示 0V 基准)
/// 注意: AC 耦合以整个传入数组为窗口估算直流分量 (非真正高通滤波器),
/// 适用于观察叠加在缓变直流上的交流分量; 若信号含大幅瞬态, 均值会被拉偏。
export function applyCoupling(
  values: Float32Array,
  coupling: Coupling,
): Float32Array {
  if (coupling === 'DC') return values;
  if (coupling === 'GND') return values.map((v) => (isNaN(v) ? NaN : 0));
  // AC: 减去非 NaN 均值
  let sum = 0;
  let n = 0;
  for (const v of values) {
    if (!isNaN(v)) { sum += v; n++; }
  }
  if (n === 0) return values;
  const mean = sum / n;
  return values.map((v) => (isNaN(v) ? NaN : v - mean));
}

/// 计算波形图水平显示窗口 (秒)
export function timeBaseToWindowSec(timeBase: number): number {
  return timeBase * H_DIVS;
}

/// 垂直 div 数 (8 div)
export const VERTICAL_DIVS = V_DIVS;
/// 水平 div 数 (10 div)
export const HORIZONTAL_DIVS = H_DIVS;
