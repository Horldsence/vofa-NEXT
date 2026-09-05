import { memo, useSyncExternalStore } from 'react';
import { Crosshair } from 'lucide-react';
import type { Lang } from '../../../i18n';
import { t } from '../../../i18n';
import type { DisplayRangeConfig } from '../../../types';
import { getAutoRangeStore } from '../../../lib/data/autoRange';
import { NumberField, SelectField } from '../../ui/fields';

interface RangeSectionProps {
  lang: Lang;
  range: DisplayRangeConfig;
  /// 自动量程订阅 key — 传控件 id (与 useAutoRange 共享同一 store)
  widgetId: string;
  onChange: (range: DisplayRangeConfig) => void;
}

/// 显示量程/刻度编辑节 — Gauge / Progress 等显示控件共用:
/// - manual: 直接编辑 min/max;
/// - auto:   滑动窗口自适应 (窗口秒数可调), 只读展示当前自适应量程,
///           可一键把当前量程捕获为手动配置。
export const RangeSection = memo(function RangeSection({ lang, range, widgetId, onChange }: RangeSectionProps) {
  // 与控件本体共享同一自动量程 store — 只读镜像当前自适应结果
  const store = getAutoRangeStore(widgetId);
  const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot);
  const hasSamples = store.sampleCount() > 0;
  const patch = (p: Partial<DisplayRangeConfig>) => onChange({ ...range, ...p });

  const commitBounds = (p: Partial<Pick<DisplayRangeConfig, 'min' | 'max'>>) => {
    const min = p.min ?? range.min;
    const max = p.max ?? range.max;
    if (!Number.isFinite(min) || !Number.isFinite(max) || max <= min) return false;
    patch(p);
    return true;
  };

  return (
    <section className="mt-3 pt-3 border-t border-border">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary mb-2">{t(lang, 'displayRange')}</div>
      <select className="form-select mb-2" value={range.mode} onChange={(e) => patch({ mode: e.target.value as DisplayRangeConfig['mode'] })}>
        <option value="manual">{t(lang, 'manual')}</option>
        <option value="auto">{t(lang, 'auto')}</option>
      </select>
      {range.mode === 'manual' ? (
        <div className="flex gap-2">
          <NumberField label={t(lang, 'minValue')} value={range.min} onCommit={(min) => commitBounds({ min })} error={t(lang, 'invalidRange')} />
          <NumberField label={t(lang, 'maxValue')} value={range.max} onCommit={(max) => commitBounds({ max })} error={t(lang, 'invalidRange')} />
        </div>
      ) : (
        <>
          <NumberField label={t(lang, 'rangeWindow')} value={range.windowSec}
            onCommit={(windowSec) => { if (windowSec >= 1 && windowSec <= 3600) { patch({ windowSec }); return true; } return false; }}
            error={t(lang, 'invalidStep')} />
          <div className="flex items-center justify-between gap-2 mb-2">
            <span className="text-[10px] text-text-secondary">{t(lang, 'autoRangeLive')}</span>
            <span className="font-mono text-[10px] text-text-primary">
              {hasSamples
                ? `[${snapshot.range.min}, ${snapshot.range.max}]`
                : `[${range.min}, ${range.max}]`}
            </span>
          </div>
          <button type="button" disabled={!hasSamples}
            className="w-full h-7 mb-1 rounded border border-border text-[11px] inline-flex items-center justify-center gap-1.5 hover:bg-bg-hover disabled:opacity-40 disabled:hover:bg-transparent transition-colors"
            onClick={() => {
              if (!hasSamples) return;
              onChange({ ...range, mode: 'manual', min: snapshot.range.min, max: snapshot.range.max });
            }}>
            <Crosshair size={12} /> {t(lang, 'captureRange')}
          </button>
        </>
      )}
      <div className="flex gap-2">
        <NumberField label={t(lang, 'majorTicks')} value={range.majorTicks}
          onCommit={(majorTicks) => {
            const n = Math.round(majorTicks);
            if (n >= 2 && n <= 11) { patch({ majorTicks: n }); return true; }
            return false;
          }}
          error={t(lang, 'invalidStep')} />
        <SelectField label={t(lang, 'tickPrecision')} value={String(range.precision)}
          options={[
            { value: 'auto', label: t(lang, 'precisionAuto') },
            ...[0, 1, 2, 3, 4, 5, 6].map((n) => ({ value: String(n), label: String(n) })),
          ]}
          onChange={(v) => patch({ precision: v === 'auto' ? 'auto' : Number(v) })} />
      </div>
    </section>
  );
});
