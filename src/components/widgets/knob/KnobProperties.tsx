import { memo } from 'react';
import { ControlRangeEditor } from '../shared/ControlRangeEditor';
import { BindingSection } from '../shared/BindingSection';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 旋钮属性 — 量程 (min/max/step) + 当前值 + 传输绑定
export const KnobProperties = memo(function KnobProperties({ widget, update }: WidgetPropertiesProps<'Knob'>) {
  return (
    <>
      <ControlRangeEditor
        params={widget.params}
        onPatch={(patch) => update({ kind: 'Knob', params: { ...widget.params, ...patch } })}
      />
      <BindingSection widget={widget} update={update} />
    </>
  );
});
