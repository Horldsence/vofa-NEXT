import { memo } from 'react';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import { TextField } from '../../ui/fields';
import { RangeSection } from '../shared/RangeSection';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 仪表盘属性 — 单位 + 量程/刻度 (手动或滑动窗口自适应)
export const GaugeProperties = memo(function GaugeProperties({ widget, update }: WidgetPropertiesProps<'Gauge'>) {
  const lang = useAppStore((s) => s.lang);
  const { range, unit } = widget.params;
  return (
    <>
      <TextField label={t(lang, 'unit')} value={unit} allowEmpty
        onCommit={(next) => update({ kind: 'Gauge', params: { ...widget.params, unit: next } })} />
      <RangeSection lang={lang} range={range} widgetId={widget.params.id}
        onChange={(nextRange) => update({ kind: 'Gauge', params: { ...widget.params, range: nextRange } })} />
    </>
  );
});
