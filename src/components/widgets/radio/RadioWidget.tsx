// ============ 单选控件 ============
//
// 纯内容组件 — 卡片 chrome (节点框/端口/删除按钮) 由 WidgetNode 提供。

import { memo, useEffect } from 'react';
import type { WidgetConfig } from '../../../types';
import { sendBindingValue } from '../shared/binding';
import { useAppStore } from '../../../store/appStore';

interface RadioProps {
  widget: Extract<WidgetConfig, { kind: 'Radio' }>;
}

export const Radio = memo(function Radio({ widget }: RadioProps) {
  const { label, options, selectedId, binding, id } = widget.params;
  const updateWidget = useAppStore((s) => s.updateWidget);
  const setInputValue = useAppStore((s) => s.setInputValue);
  const selected = options.find((option) => option.id === selectedId) ?? options[0];
  const value = selected?.value ?? 0;

  const select = (optionId: string) => {
    const option = options.find((item) => item.id === optionId);
    if (!option || optionId === selectedId) return;
    updateWidget(id, { kind: 'Radio', params: { ...widget.params, selectedId: optionId } });
    setInputValue(id, option.value);
    sendBindingValue(binding, option.value);
  };

  useEffect(() => { setInputValue(id, value); }, [id, setInputValue, value]);

  return (
    <div className="nodrag nowheel flex flex-col gap-1" role="radiogroup" aria-label={label}>
      {options.map((option) => (
        <label key={option.id} className="nodrag nowheel flex items-center gap-1.5 cursor-pointer text-xs">
          <input
            type="radio"
            name={id}
            checked={selected?.id === option.id}
            onChange={() => select(option.id)}
            className="nodrag nowheel accent-accent"
          />
          <span className="truncate" title={option.label}>{option.label}</span>
        </label>
      ))}
    </div>
  );
});
