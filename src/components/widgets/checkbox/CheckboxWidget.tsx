// ============ 复选控件 ============
//
// 纯内容组件 — 卡片 chrome (节点框/端口/删除按钮) 由 WidgetNode 提供。

import { memo, useEffect } from 'react';
import type { WidgetConfig } from '../../../types';
import { sendBindingValue } from '../shared/binding';
import { useAppStore } from '../../../store/appStore';
import { widgetInputValue } from '../../../lib/utils/widgetDefaults';

interface CheckboxProps {
  widget: Extract<WidgetConfig, { kind: 'Checkbox' }>;
}

export const Checkbox = memo(function Checkbox({ widget }: CheckboxProps) {
  const { label, options, selectedIds, binding, id } = widget.params;
  const updateWidget = useAppStore((s) => s.updateWidget);
  const setInputValue = useAppStore((s) => s.setInputValue);
  const value = widgetInputValue(widget) ?? 0;

  const toggle = (optionId: string) => {
    const selected = new Set(selectedIds);
    if (selected.has(optionId)) selected.delete(optionId);
    else selected.add(optionId);
    const nextWidget: Extract<WidgetConfig, { kind: 'Checkbox' }> = {
      kind: 'Checkbox',
      params: { ...widget.params, selectedIds: [...selected] },
    };
    const nextValue = widgetInputValue(nextWidget) ?? 0;
    updateWidget(id, nextWidget);
    setInputValue(id, nextValue);
    sendBindingValue(binding, nextValue);
  };

  useEffect(() => { setInputValue(id, value); }, [id, setInputValue, value]);

  return (
    <div className="nodrag nowheel flex flex-col gap-1" role="group" aria-label={label}>
      {options.map((option) => (
        <label key={option.id} className="nodrag nowheel flex items-center gap-1.5 cursor-pointer text-xs">
          <input
            type="checkbox"
            checked={selectedIds.includes(option.id)}
            onChange={() => toggle(option.id)}
            className="nodrag nowheel accent-accent"
          />
          <span className="truncate" title={option.label}>{option.label}</span>
        </label>
      ))}
    </div>
  );
});
