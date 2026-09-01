import { useEffect } from 'react';
import type { WidgetConfig } from '../../types';
import { sendBindingValue } from './binding';
import { useAppStore } from '../../store/appStore';
import { WidgetCard } from '../ui/WidgetCard';
import { widgetInputValue } from '../../lib/utils/createWidget';

interface CheckboxProps {
  widget: Extract<WidgetConfig, { kind: 'Checkbox' }>;
  onRemove: () => void;
}

export function Checkbox({ widget, onRemove }: CheckboxProps) {
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
    <WidgetCard label={label} onRemove={onRemove}>
      <div className="nodrag nowheel flex flex-col gap-1">
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
    </WidgetCard>
  );
}
