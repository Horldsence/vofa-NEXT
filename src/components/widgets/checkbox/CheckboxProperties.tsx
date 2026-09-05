// ============ 复选属性 ============

import { memo } from 'react';
import { ChoiceSection } from '../shared/ChoiceSection';
import { BindingSection } from '../shared/BindingSection';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 复选属性 — 选项编辑 + 传输绑定
export const CheckboxProperties = memo(function CheckboxProperties({ widget, update }: WidgetPropertiesProps<'Checkbox'>) {
  return (
    <>
      <ChoiceSection widget={widget} update={update} />
      <BindingSection widget={widget} update={update} />
    </>
  );
});
