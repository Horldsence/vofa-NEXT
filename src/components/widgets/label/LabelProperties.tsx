// ============ 标签属性 ============

import { memo } from 'react';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import { TextField } from '../../ui/fields';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 标签属性 — 文本内容
export const LabelProperties = memo(function LabelProperties({ widget, update }: WidgetPropertiesProps<'Label'>) {
  const lang = useAppStore((s) => s.lang);
  const patch = (p: Partial<typeof widget.params>) => update({ kind: 'Label', params: { ...widget.params, ...p } });
  return (
    <TextField label={t(lang, 'labelText')} value={widget.params.text} onCommit={(text) => patch({ text })} />
  );
});
