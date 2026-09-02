import {
  TIME_BASES_SEC,
  type Coupling,
  type ScopeAxisConfig,
  type ScopeMeasurements,
  type WaveformWindow,
} from '../../types';

const H_DIVS = 10;
const V_DIVS = 8;

/// V/div 取档 — 向上取最近 1-2-5 档 (不限于 V_PER_DIV 表, 可跨任意数量级)
/// 向上取保证信号跨度始终 <= V_DIVS * FILL_RATIO 格, 不会顶出屏幕;
/// 小信号 (如 Vpp < 1mV) 会取到表外的 µV/nV 档, 避免压成一条直线
export function snapVPerDivUp(target: number): number {
  if (!isFinite(target) || target <= 0) return 1;
  const decade = Math.pow(10, Math.floor(Math.log10(target)));
  const m = target / decade;
  // 容差抵消浮点误差 (如 0.3/0.1 = 2.9999999999999996)
  const mantissa = m <= 1 + 1e-9 ? 1 : m <= 2 + 1e-9 ? 2 : m <= 5 + 1e-9 ? 5 : 10;
  return mantissa * decade;
}

/// 耦合方式数据变换 - 对原始通道数据应用 DC/AC/GND 耦合
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

/// 计算单通道测量值 (Vpp/Vmin/Vmax/Vavg/Vrms/Freq)
export function computeMeasurements(
  values: Float32Array | number[],
  timestampsMs: Float64Array | number[]
): ScopeMeasurements | null {
  if (values.length < 2) return null;
  let vmin = Infinity;
  let vmax = -Infinity;
  let sum = 0;
  let sqSum = 0;
  let n = 0;
  for (const v of values) {
    if (isNaN(v)) continue;
    if (v < vmin) vmin = v;
    if (v > vmax) vmax = v;
    sum += v;
    sqSum += v * v;
    n++;
  }
  if (n === 0 || vmin === Infinity) return null;
  const vavg = sum / n;
  const vrms = Math.sqrt(Math.max(0, sqSum / n - vavg * vavg));
  const vpp = vmax - vmin;

  // 频率估算: 零交叉检测
  let freq: number | null = null;
  let period: number | null = null;
  if (vpp > 1e-9 && timestampsMs.length >= 3) {
    const threshold = vavg;
    let zeroCrossings = 0;
    let lastDir = 0;
    let firstCrossing = -1;
    let lastCrossing = -1;
    for (let i = 1; i < values.length; i++) {
      const prev = values[i - 1];
      const curr = values[i];
      if (isNaN(prev) || isNaN(curr)) continue;
      const dir = curr > threshold ? 1 : -1;
      if (lastDir !== 0 && dir !== lastDir) {
        if (firstCrossing < 0) firstCrossing = i;
        lastCrossing = i;
        zeroCrossings++;
      }
      lastDir = dir;
    }
    if (zeroCrossings >= 2 && lastCrossing > firstCrossing) {
      const dt = (timestampsMs[lastCrossing] - timestampsMs[firstCrossing]) / 1000;
      if (dt > 0) {
        period = (dt * 2) / zeroCrossings;
        freq = 1 / period;
      }
    }
  }

  return { vpp, vmin, vmax, vavg, vrms, freq, period };
}

/// Auto Set: 基于 waveformWindow 数据自动适配时基与每通道 V/div
/// 信号垂直方向约占 70% (上下各留 ~15% 余量), 避免完全顶满
export function computeAutoSetConfig(
  win: WaveformWindow,
  currentConfig: ScopeAxisConfig,
  connectedChannels: number[]
): ScopeAxisConfig {
  if (win.timestamps.length < 2) return currentConfig;

  const firstTs = win.timestamps[0];
  const lastTs = win.timestamps[win.timestamps.length - 1];
  const totalDurSec = (lastTs - firstTs) / 1000;
  if (totalDurSec <= 0) return currentConfig;

  // 时基: 总时长 / 10 格
  const targetTb = totalDurSec / H_DIVS;
  let bestTbIdx = 0;
  let bestTbDiff = Infinity;
  for (let i = 0; i < TIME_BASES_SEC.length; i++) {
    const diff = Math.abs(TIME_BASES_SEC[i] - targetTb);
    if (diff < bestTbDiff) {
      bestTbDiff = diff;
      bestTbIdx = i;
    }
  }
  const newTimeBase = TIME_BASES_SEC[bestTbIdx];

  // 每通道 V/div
  const channelsToUse =
    connectedChannels.length > 0
      ? connectedChannels
      : Array.from({ length: win.channel_count }, (_, i) => i);

  // 信号目标占垂直方向的比例 (70%), 上下各留 15% 余量
  const VERTICAL_FILL_RATIO = 0.7;

  const newChannels = currentConfig.channels.slice();
  // 补齐 channels 数组到 channel_count
  while (newChannels.length < win.channel_count) {
    newChannels.push({ vPerDiv: 1, position: 0, show: true, coupling: 'DC' as const });
  }

  if (currentConfig.sharedY) {
    // 共用 Y 模式: 计算所有连接通道的全局 min/max, 设置单一 vPerDiv/position 到 channels[0]
    let globalMin = Infinity;
    let globalMax = -Infinity;
    for (const chIdx of channelsToUse) {
      const ch = win.channels[chIdx];
      if (!ch || ch.length === 0) continue;
      for (const v of ch) {
        if (isNaN(v)) continue;
        if (v < globalMin) globalMin = v;
        if (v > globalMax) globalMax = v;
      }
    }
    if (globalMin !== Infinity) {
      const vpp = globalMax - globalMin;
      newChannels[0] = {
        ...newChannels[0],
        // vpp=0 (平直信号) 时保持当前 vPerDiv, 仅居中
        vPerDiv: vpp > 0 ? snapVPerDivUp(vpp / (V_DIVS * VERTICAL_FILL_RATIO)) : newChannels[0].vPerDiv,
        position: (globalMax + globalMin) / 2,
      };
    }
  } else {
    // 独立 Y 模式: 每通道独立计算 vPerDiv/position
    for (const chIdx of channelsToUse) {
      const ch = win.channels[chIdx];
      if (!ch || ch.length === 0) continue;
      let vmin = Infinity;
      let vmax = -Infinity;
      for (const v of ch) {
        if (isNaN(v)) continue;
        if (v < vmin) vmin = v;
        if (v > vmax) vmax = v;
      }
      if (vmin === Infinity) continue;
      const vpp = vmax - vmin;
      while (newChannels.length <= chIdx) {
        newChannels.push({
          vPerDiv: 1,
          position: 0,
          show: true,
          coupling: 'DC' as const,
        });
      }
      newChannels[chIdx] = {
        ...newChannels[chIdx],
        // 信号 Vpp 占满 V_DIVS * FILL_RATIO 格; vpp=0 (平直信号) 保持当前 vPerDiv, 仅居中
        vPerDiv: vpp > 0 ? snapVPerDivUp(vpp / (V_DIVS * VERTICAL_FILL_RATIO)) : newChannels[chIdx].vPerDiv,
        // position 取信号中点, 让信号居中显示
        position: (vmax + vmin) / 2,
      };
    }
  }

  return {
    ...currentConfig,
    timeBase: newTimeBase,
    channels: newChannels,
    hPosition: 0,
    running: true,
  };
}

/// 计算波形图水平显示窗口 (秒)
export function timeBaseToWindowSec(timeBase: number): number {
  return timeBase * H_DIVS;
}

/// 垂直 div 数 (8 div)
export const VERTICAL_DIVS = V_DIVS;
/// 水平 div 数 (10 div)
export const HORIZONTAL_DIVS = H_DIVS;
