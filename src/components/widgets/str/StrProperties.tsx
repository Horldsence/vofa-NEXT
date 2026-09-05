// ============ 字符串操作属性 ============

import { memo } from 'react';
import { STR_OP_PORTS } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import { NumberField, TextField } from '../../ui/fields';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 字符串操作属性 — tmpl 模板 (format) 与 pos/len/size 内联回退值。
/// 数值框是「端口未连接时的回退值」; 端口已连接时后端取上游值 (节点内原为禁用态, 面板中始终可编辑)。
export const StrProperties = memo(function StrProperties({ widget, update }: WidgetPropertiesProps<'Str'>) {
  const lang = useAppStore((s) => s.lang);
  const { id, op, tmpl, pos, len, size } = widget.params;
  const meta = STR_OP_PORTS[op];
  const patch = (p: Partial<typeof widget.params>) => update({ kind: 'Str', params: { ...widget.params, ...p } });
  const INLINE_LABEL: Record<string, string> = { pos: 'strPortPos', len: 'strPortLen', size: 'strPortSize' };
  const VALUES = { pos, len, size } as const;
  return (
    <section className="mt-3 pt-3 border-t border-border">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary mb-2">{t(lang, 'strSettings')}</div>
      {op === 'format' && (
        <TextField label={t(lang, 'strFormatTmpl')} value={tmpl ?? ''}
          onCommit={(next) => patch({ tmpl: next })} />
      )}
      {meta.inlineNumPorts.map((portId) => (
        <NumberField key={portId} label={t(lang, INLINE_LABEL[portId] ?? portId)}
          value={VALUES[portId as 'pos' | 'len' | 'size']}
          onCommit={(v) => { if (v >= 0) { patch({ [portId]: Math.round(v) }); return true; } return false; }}
          error={t(lang, 'invalidStep')} />
      ))}
      <div className="text-[10px] text-text-secondary">{t(lang, 'strInlineFallbackHint')}</div>
      <div className="mt-1 text-[10px] text-text-muted break-all">id: {id}</div>
    </section>
  );
});
