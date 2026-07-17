import {
  TIME_BASES_SEC,
  V_PER_DIV,
  type ScopeAxisConfig,
  type ScopeMeasurements,
  type WaveformWindow,
} from '../types';

const H_DIVS = 10;
const V_DIVS = 8;

/// 计算单通道测量值 (Vpp/Vmin/Vmax/Vavg/Vrms/Freq)
export function computeMeasurements(
  values: number[],
  timestampsMs: number[]
): ScopeMeasurements | null {
  if (values.length < 2) return null;
  let vmin = Infinity;
  let vmax = -Infinity;
  let sum = 0;
  let sqSum = 0;
  let n = 0;
  for (let i = 0; i < values.length; i++) {
    const v = values[i];
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

  const newChannels = currentConfig.channels.slice();
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
    const targetVd = (vmax - vmin) / V_DIVS;
    let bestVdIdx = 0;
    let bestVdDiff = Infinity;
    for (let i = 0; i < V_PER_DIV.length; i++) {
      const diff = Math.abs(V_PER_DIV[i] - targetVd);
      if (diff < bestVdDiff) {
        bestVdDiff = diff;
        bestVdIdx = i;
      }
    }
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
      vPerDiv: V_PER_DIV[bestVdIdx],
      position: (vmax + vmin) / 2,
    };
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
