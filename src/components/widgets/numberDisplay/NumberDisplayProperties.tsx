// ============ 大数字显示属性编辑节 ============
//
// 单位 (可空) + 小数位 (0..6)。

import { memo } from 'react';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import { NumberField, TextField } from '../../ui/fields';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 大数字显示属性 — 单位 + 小数位
export const NumberDisplayProperties = memo(function NumberDisplayProperties({ widget, update }: WidgetPropertiesProps<'NumberDisplay'>) {
  const lang = useAppStore((s) => s.lang);
  const { unit, precision } = widget.params;
  const patch = (p: Partial<typeof widget.params>) =>
    update({ kind: 'NumberDisplay', params: { ...widget.params, ...p } });
  return (
    <>
      <TextField label={t(lang, 'unit')} value={unit} allowEmpty
        onCommit={(next) => patch({ unit: next })} />
      <NumberField label={t(lang, 'precision')} value={precision}
        onCommit={(v) => {
          const next = Math.round(v);
          if (next < 0 || next > 6) return false;
          patch({ precision: next });
          return true;
        }}
        error={t(lang, 'invalidRange')} />
    </>
  );
});
