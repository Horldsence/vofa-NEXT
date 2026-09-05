import { memo } from 'react';
import { ControlRangeEditor } from '../shared/ControlRangeEditor';
import { BindingSection } from '../shared/BindingSection';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 滑块属性 — 量程 (min/max/step) + 当前值 + 传输绑定
export const SliderProperties = memo(function SliderProperties({ widget, update }: WidgetPropertiesProps<'Slider'>) {
  return (
    <>
      <ControlRangeEditor
        params={widget.params}
        onPatch={(patch) => update({ kind: 'Slider', params: { ...widget.params, ...patch } })}
      />
      <BindingSection widget={widget} update={update} />
    </>
  );
});
