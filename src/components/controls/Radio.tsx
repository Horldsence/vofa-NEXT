import { useEffect } from 'react';
import type { WidgetConfig } from '../../types';
import { sendBindingValue } from './binding';
import { useAppStore } from '../../store/appStore';
import { WidgetCard } from '../ui/WidgetCard';

interface RadioProps {
  widget: Extract<WidgetConfig, { kind: 'Radio' }>;
  onRemove: () => void;
}

export function Radio({ widget, onRemove }: RadioProps) {
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
    <WidgetCard label={label} onRemove={onRemove}>
      <div className="nodrag nowheel flex flex-col gap-1">
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
    </WidgetCard>
  );
}
