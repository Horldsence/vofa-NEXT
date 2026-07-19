import { useMemo } from 'react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import type { CanLoadSnapshot, CanIdLoadHistory } from '../../types';

/// 窗口预设
export const WINDOW_PRESETS: { label: string; value: number }[] = [
  { label: '100ms', value: 100_000 },
  { label: '500ms', value: 500_000 },
  { label: '1s', value: 1_000_000 },
  { label: '5s', value: 5_000_000 },
  { label: '10s', value: 10_000_000 },
];

/// 波特率预设
export const BITRATE_PRESETS: { label: string; value: number }[] = [
  { label: '100k', value: 100_000 },
  { label: '125k', value: 125_000 },
  { label: '250k', value: 250_000 },
  { label: '500k', value: 500_000 },
  { label: '1M', value: 1_000_000 },
];

/// 告警阈值预设
export const THRESHOLD_PRESETS: { label: string; value: number }[] = [
  { label: '50%', value: 0.5 },
  { label: '60%', value: 0.6 },
  { label: '70%', value: 0.7 },
  { label: '80%', value: 0.8 },
  { label: '90%', value: 0.9 },
];

/// 负载率颜色 (绿 → 黄 → 红)
export function loadColor(ratio: number): string {
  if (ratio < 0.6) return '#89d185';
  if (ratio < 0.85) return '#d1c04d';
  return '#d18585';
}

/// 格式化百分比
export function formatPercent(ratio: number): string {
  return (ratio * 100).toFixed(2) + '%';
}

/// 格式化帧率
export function formatFps(fps: number): string {
  return fps.toFixed(1) + ' fps';
}

/// 格式化波特率
export function formatBitrate(bps: number): string {
  if (bps >= 1_000_000) return `${bps / 1_000_000} Mbps`;
  return `${bps / 1000} kbps`;
}

/// 从 snapshot.per_id_history 中查找指定 ID 的历史
export function findIdHistory(
  snapshot: CanLoadSnapshot | null,
  sel: { id: number; extended: boolean }
): CanIdLoadHistory | null {
  if (!snapshot) return null;
  return (
    snapshot.per_id_history.find((h) => h.id === sel.id && h.extended === sel.extended) ?? null
  );
}

/// ID 负载分布
export function IdLoadDistribution({
  snapshot,
  selectedId,
  onSelectId,
}: {
  snapshot: CanLoadSnapshot | null;
  selectedId: { id: number; extended: boolean } | null;
  onSelectId: (id: number, extended: boolean) => void;
}) {
  const lang = useAppStore.getState().lang;
  const perId = snapshot?.per_id ?? [];
  const maxBits = perId.length > 0 ? perId[0].total_bits : 1;

  const totalBits = useMemo(() => perId.reduce((sum, s) => sum + s.total_bits, 0), [perId]);

  return (
    <div className="bg-bg-panel-header rounded border border-border p-3">
      <div className="flex items-center justify-between mb-3">
        <span className="text-xs text-text-secondary uppercase tracking-wider">
          {t(lang, 'canLoadPerId')}
        </span>
        <span className="text-xs text-text-secondary font-mono">
          {perId.length} IDs{selectedId ? ` · ${t(lang, 'canLoadIdFilterHint')}` : ''}
        </span>
      </div>
      {perId.length === 0 ? (
        <div className="flex items-center justify-center h-32 text-text-secondary text-xs rounded border border-dashed border-border">
          {t(lang, 'noCanFrames')}
        </div>
      ) : (
        <div className="space-y-1.5 max-h-[280px] overflow-y-auto">
          {perId.map((s) => {
            const pct = (s.total_bits / maxBits) * 100;
            const sharePct = totalBits > 0 ? (s.total_bits / totalBits) * 100 : 0;
            const isSelected =
              selectedId !== null &&
              selectedId.id === s.id &&
              selectedId.extended === s.extended;
            return (
              <div
                key={`${s.id}-${s.extended}`}
                className={`grid grid-cols-[6rem_1fr_4rem] items-center gap-3 text-xs font-mono cursor-pointer rounded px-1 py-0.5 transition-colors ${
                  isSelected
                    ? 'bg-accent/20 border border-accent'
                    : 'border border-transparent hover:bg-bg-hover'
                } ${selectedId && !isSelected ? 'opacity-50' : ''}`}
                onClick={() => onSelectId(s.id, s.extended)}
                title={t(lang, 'canLoadIdFilterHint')}
              >
                <span className="text-text-bright truncate">
                  0x{s.extended
                    ? s.id.toString(16).toUpperCase().padStart(8, '0')
                    : s.id.toString(16).toUpperCase().padStart(3, '0')}
                  {s.extended && <span className="ml-1 text-accent text-[10px]">X</span>}
                </span>
                <div className="bg-bg-input rounded h-5 overflow-hidden relative">
                  <div
                    className={`absolute inset-y-0 left-0 ${isSelected ? 'bg-orange/80' : 'bg-blue/70'}`}
                    style={{ width: `${pct}%` }}
                  />
                  <div className="absolute inset-0 flex items-center px-2 text-[10px] text-text-bright gap-2">
                    <span>{s.frame_count}f</span>
                    <span className="text-text-secondary">{sharePct.toFixed(1)}%</span>
                  </div>
                </div>
                <span className="text-text-secondary text-right">{s.total_bits}b</span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
