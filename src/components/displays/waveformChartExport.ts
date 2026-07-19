import { waveformWindow } from '../../lib/dataBuffer';
import { getEffectiveChannel } from '../../types';
import type { ScopeAxisConfig } from '../../types';

/** 最小 slot 接口 — 兼容 SeriesSlot 结构 */
interface ExportSlot {
  input: { kind: string; idx?: number; sourceId?: string; sourceHandle?: string };
  cfgIdx: number;
  label?: string;
}

/// 获取当前可用于导出的原始数据 (真实值, 相对秒)
export function getExportData(
  cfg: ScopeAxisConfig,
  slots: ExportSlot[],
  widgetId: string,
  frozenData: { ts: number[]; chs: number[][]; derived?: Record<string, Record<string, number[]>> } | null,
): { tsSec: number[]; series: number[][]; labels: string[] } {
  let timestamps: number[];
  let channelArrays: number[][];
  let derivedMap: Record<string, Record<string, number[]>> | undefined;
  let baseTs: number;

  if (cfg.running) {
    const win = waveformWindow.get();
    timestamps = win.timestamps.map((t) => t);
    channelArrays = win.channels;
    derivedMap = win.derived;
    baseTs = 0;
  } else {
    if (!frozenData || frozenData.ts.length === 0) {
      return { tsSec: [], series: [], labels: [] };
    }
    timestamps = frozenData.ts.map((t) => t);
    channelArrays = frozenData.chs;
    derivedMap = frozenData.derived;
    baseTs = frozenData.ts[0];
  }

  const tsSec = timestamps.map((ms) => (ms - baseTs) / 1000);

  const series: number[][] = [];
  const labels: string[] = [];
  for (const slot of slots) {
    let arr: number[] | undefined;
    if (slot.input.kind === 'channel' && slot.input.idx != null) {
      arr = channelArrays[slot.input.idx];
    } else if (slot.input.kind === 'derived') {
      arr = derivedMap?.[widgetId]?.[slot.input.sourceId ?? ''];
    }
    if (!arr) continue;
    const realArr = arr.map((v) => {
      if (isNaN(v)) return NaN;
      if (cfg.sharedY) return v;
      const eff = getEffectiveChannel(cfg, slot.cfgIdx);
      return v * eff.vPerDiv + eff.position;
    });
    series.push(realArr);
    labels.push(slot.label ?? '');
  }

  return { tsSec, series, labels };
}

export function buildCsvForRange(
  range: { startSec: number; endSec: number },
  exportData: { tsSec: number[]; series: number[][]; labels: string[] },
): string {
  const { tsSec, series, labels } = exportData;
  if (tsSec.length === 0 || series.length === 0) return '';
  const start = Math.min(range.startSec, range.endSec);
  const end = Math.max(range.startSec, range.endSec);

  let startIdx = tsSec.length;
  for (let i = 0; i < tsSec.length; i++) {
    if (tsSec[i] >= start) { startIdx = i; break; }
  }
  let endIdx = tsSec.length;
  for (let i = startIdx; i < tsSec.length; i++) {
    if (tsSec[i] > end) { endIdx = i; break; }
  }
  if (startIdx >= endIdx) return '';

  const rows: string[] = [];
  rows.push(['Time(s)', ...labels].join(','));
  for (let i = startIdx; i < endIdx; i++) {
    const row = [
      tsSec[i].toFixed(6),
      ...series.map((s) => (isNaN(s[i]) ? '' : s[i].toExponential(6))),
    ];
    rows.push(row.join(','));
  }
  return rows.join('\n');
}
