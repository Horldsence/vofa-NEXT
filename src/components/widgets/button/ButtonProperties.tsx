// ============ 按钮属性 ============

import { memo } from 'react';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import { NumberField } from '../../ui/fields';
import { BindingSection } from '../shared/BindingSection';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 按钮属性 — 按下/释放值 + 传输绑定
export const ButtonProperties = memo(function ButtonProperties({ widget, update }: WidgetPropertiesProps<'Button'>) {
  const lang = useAppStore((s) => s.lang);
  const patch = (p: Partial<typeof widget.params>) => update({ kind: 'Button', params: { ...widget.params, ...p } });
  return (
    <>
      <NumberField label={t(lang, 'press')} value={widget.params.pressValue}
        onCommit={(pressValue) => { patch({ pressValue }); return true; }} />
      <NumberField label={t(lang, 'release')} value={widget.params.releaseValue}
        onCommit={(releaseValue) => { patch({ releaseValue }); return true; }} />
      <BindingSection widget={widget} update={update} />
    </>
  );
});
