// ============ 文本下发属性 ============

import { memo } from 'react';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import { NumberField, SelectField } from '../../ui/fields';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 文本下发属性 — 目标串口 / 换行 / 限速 (卡片内只保留预览与发送按钮)
export const TextOutProperties = memo(function TextOutProperties({ widget, update }: WidgetPropertiesProps<'TextOut'>) {
  const lang = useAppStore((s) => s.lang);
  const nodes = useAppStore((s) => s.rfNodes);
  const { targetTransport, newline, minIntervalMs } = widget.params;
  const patch = (p: Partial<typeof widget.params>) => update({ kind: 'TextOut', params: { ...widget.params, ...p } });
  const transports = nodes.filter((n) => n.type === 'transport');
  return (
    <section className="mt-3 pt-3 border-t border-border">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary mb-2">{t(lang, 'textOutSettings')}</div>
      <SelectField label={t(lang, 'textOutTarget')} value={targetTransport}
        options={[
          { value: '', label: t(lang, 'textOutNoTarget') },
          ...transports.map((n) => ({
            value: n.id,
            label: typeof n.data?.label === 'string' && n.data.label ? n.data.label : n.id,
          })),
        ]}
        onChange={(v) => patch({ targetTransport: v })} />
      <SelectField label={t(lang, 'textOutNewline')} value={newline}
        options={([
          ['none', 'textOutNlNone'],
          ['lf', 'textOutNlLf'],
          ['crlf', 'textOutNlCrlf'],
          ['cr', 'textOutNlCr'],
        ] as [typeof newline, string][]).map(([value, key]) => ({ value, label: t(lang, key) }))}
        onChange={(v) => patch({ newline: v as typeof newline })} />
      <NumberField label={`${t(lang, 'textOutInterval')} (ms)`} value={minIntervalMs}
        onCommit={(v) => { if (v >= 0) { patch({ minIntervalMs: Math.round(v) }); return true; } return false; }}
        error={t(lang, 'invalidStep')} />
    </section>
  );
});
