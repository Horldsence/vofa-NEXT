import { memo } from 'react';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import { TextField } from '../../ui/fields';
import { RangeSection } from '../shared/RangeSection';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 进度条属性 — 单位/方向/显示数值/填充色 + 量程刻度
export const ProgressProperties = memo(function ProgressProperties({ widget, update }: WidgetPropertiesProps<'Progress'>) {
  const lang = useAppStore((s) => s.lang);
  const { range, unit, orientation, showValue, color } = widget.params;
  const patch = (p: Partial<typeof widget.params>) =>
    update({ kind: 'Progress', params: { ...widget.params, ...p } });
  return (
    <>
      <TextField label={t(lang, 'unit')} value={unit} allowEmpty
        onCommit={(next) => patch({ unit: next })} />
      <div className="flex gap-2">
        <label className="block mb-2 flex-1 min-w-0">
          <span className="block text-xs text-text-secondary mb-1">{t(lang, 'orientation')}</span>
          <select className="form-select" value={orientation}
            onChange={(e) => patch({ orientation: e.target.value as typeof orientation })}>
            <option value="horizontal">{t(lang, 'orientationHorizontal')}</option>
            <option value="vertical">{t(lang, 'orientationVertical')}</option>
          </select>
        </label>
        <label className="block mb-2 flex-1 min-w-0">
          <span className="block text-xs text-text-secondary mb-1">{t(lang, 'fillColor')}</span>
          <span className="flex items-center gap-1.5">
            <input type="color" className="h-[30px] w-9 cursor-pointer rounded border border-border bg-transparent p-0.5"
              value={color || '#75beff'}
              onChange={(e) => patch({ color: e.target.value })} aria-label={t(lang, 'fillColor')} />
            <button type="button" title={t(lang, 'precisionAuto')}
              className="h-[30px] px-1.5 text-[10px] text-text-secondary rounded border border-border hover:bg-bg-hover transition-colors"
              onClick={() => patch({ color: '' })}>×</button>
          </span>
        </label>
      </div>
      <label className="flex items-center gap-2 mb-2 text-xs text-text-secondary">
        <input type="checkbox" checked={showValue} onChange={(e) => patch({ showValue: e.target.checked })} />
        {t(lang, 'showValue')}
      </label>
      <RangeSection lang={lang} range={range} widgetId={widget.params.id}
        onChange={(nextRange) => patch({ range: nextRange })} />
    </>
  );
});
