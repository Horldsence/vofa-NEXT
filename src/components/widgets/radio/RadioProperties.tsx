// ============ 单选属性 ============

import { memo } from 'react';
import { ChoiceSection } from '../shared/ChoiceSection';
import { BindingSection } from '../shared/BindingSection';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 单选属性 — 选项编辑 + 传输绑定
export const RadioProperties = memo(function RadioProperties({ widget, update }: WidgetPropertiesProps<'Radio'>) {
  return (
    <>
      <ChoiceSection widget={widget} update={update} />
      <BindingSection widget={widget} update={update} />
    </>
  );
});
