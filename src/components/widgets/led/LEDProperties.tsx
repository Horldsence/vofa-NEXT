// ============ LED 属性编辑节 ============
//
// 阈值 + ON/OFF 颜色 — 颜色输入沿用 ProgressProperties 的 color input 模式。

import { memo } from 'react';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import { NumberField } from '../../ui/fields';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 单个颜色选择行 — input type="color" + aria-label
function ColorField({ label, value, onChange }: { label: string; value: string; onChange: (next: string) => void }) {
  return (
    <label className="block mb-2">
      <span className="block text-xs text-text-secondary mb-1">{label}</span>
      <input type="color" className="h-[30px] w-9 cursor-pointer rounded border border-border bg-transparent p-0.5"
        value={value} onChange={(e) => onChange(e.target.value)} aria-label={label} />
    </label>
  );
}

/// LED 属性 — 阈值 + ON/OFF 颜色
export const LEDProperties = memo(function LEDProperties({ widget, update }: WidgetPropertiesProps<'LED'>) {
  const lang = useAppStore((s) => s.lang);
  const { threshold, on_color, off_color } = widget.params;
  const patch = (p: Partial<typeof widget.params>) =>
    update({ kind: 'LED', params: { ...widget.params, ...p } });
  return (
    <>
      <NumberField label={t(lang, 'ledThreshold')} value={threshold}
        onCommit={(v) => { patch({ threshold: v }); return true; }} />
      <ColorField label={t(lang, 'ledOnColor')} value={on_color} onChange={(next) => patch({ on_color: next })} />
      <ColorField label={t(lang, 'ledOffColor')} value={off_color} onChange={(next) => patch({ off_color: next })} />
    </>
  );
});
