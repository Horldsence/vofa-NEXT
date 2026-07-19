import { useEffect } from 'react';
import type { WidgetConfig } from '../../types';
import { sendBindingValue } from './binding';
import { useAppStore } from '../../store/appStore';
import { WidgetCard } from '../ui/WidgetCard';

interface CheckboxProps {
  widget: Extract<WidgetConfig, { kind: 'Checkbox' }>;
  onRemove: () => void;
}

/// 多选控件 — 切换时发送 checked_value 或 unchecked_value
/// 当前值通过 setInputValue 推送到后端图 (事件驱动, 供下游 widget 读取)
export function Checkbox({ widget, onRemove }: CheckboxProps) {
  const { label, checked_value, unchecked_value, binding, id } = widget.params;
  const checked = useAppStore((s) => {
    const w = s.widgets.find((w) => w.params.id === widget.params.id);
    if (w && w.kind === 'Checkbox') return w.params.default;
    return widget.params.default;
  });
  const updateWidget = useAppStore((s) => s.updateWidget);
  const setInputValue = useAppStore((s) => s.setInputValue);
  const value = checked ? checked_value : unchecked_value;

  const handleToggle = () => {
    const next = !checked;
    updateWidget(widget.params.id, {
      kind: 'Checkbox',
      params: { ...widget.params, default: next },
    });
    sendBindingValue(binding, next ? checked_value : unchecked_value);
  };

  // 同步当前值到后端图 (事件驱动)
  useEffect(() => {
    setInputValue(id, value);
  }, [id, value, setInputValue]);

  return (
    <WidgetCard label={label} onRemove={onRemove}>
      <label className="flex items-center gap-1.5 cursor-pointer text-xs">
        <input type="checkbox" checked={checked} onChange={handleToggle} className="accent-accent" />
        <span>
          {checked ? checked_value : unchecked_value}
        </span>
      </label>
    </WidgetCard>
  );
}
