// ============ 滤波器属性编辑节 ============
//
// 从 WidgetProperties 的 FilterSettings 内联实现迁移至 per-kind 注册表。

import { memo } from 'react';
import type { FilterPresetKind } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import { NumberField, SelectField } from '../../ui/fields';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 滤波器参数 — 预设 / 截止 / 带宽 / 采样率
export const FilterProperties = memo(function FilterProperties({ widget, update }: WidgetPropertiesProps<'Filter'>) {
  const lang = useAppStore((s) => s.lang);
  const { preset, cutoff, low, high, sampleRate } = widget.params;
  const patch = (p: Partial<typeof widget.params>) =>
    update({ kind: 'Filter', params: { ...widget.params, ...p } });
  return (
    <section className="mt-3 pt-3 border-t border-border">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary mb-2">{t(lang, 'filterSettings')}</div>
      <SelectField label={t(lang, 'filterPreset')} value={preset}
        options={([
          ['Lowpass', 'filterLowpass'],
          ['Highpass', 'filterHighpass'],
          ['Bandpass', 'filterBandpass'],
          ['Bandstop', 'filterBandstop'],
        ] as [FilterPresetKind, string][]).map(([value, key]) => ({ value, label: t(lang, key) }))}
        onChange={(v) => patch({ preset: v as FilterPresetKind })} />
      {(preset === 'Lowpass' || preset === 'Highpass') && (
        <NumberField label={`${t(lang, 'filterCutoff')} (Hz)`} value={cutoff}
          onCommit={(v) => { if (v > 0) { patch({ cutoff: v }); return true; } return false; }}
          error={t(lang, 'invalidStep')} />
      )}
      {(preset === 'Bandpass' || preset === 'Bandstop') && (
        <>
          <NumberField label={`${t(lang, 'filterLow')} (Hz)`} value={low}
            onCommit={(v) => { if (v > 0) { patch({ low: v }); return true; } return false; }}
            error={t(lang, 'invalidStep')} />
          <NumberField label={`${t(lang, 'filterHigh')} (Hz)`} value={high}
            onCommit={(v) => { if (v > 0) { patch({ high: v }); return true; } return false; }}
            error={t(lang, 'invalidStep')} />
        </>
      )}
      <NumberField label={`${t(lang, 'filterSampleRate')} (Hz)`} value={sampleRate}
        onCommit={(v) => { if (v > 0) { patch({ sampleRate: v }); return true; } return false; }}
        error={t(lang, 'invalidStep')} />
    </section>
  );
});
