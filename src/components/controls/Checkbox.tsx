import { X } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { sendBindingValue } from './binding';
import { useAppStore } from '../../store/appStore';

interface CheckboxProps {
  widget: Extract<WidgetConfig, { kind: 'Checkbox' }>;
  onRemove: () => void;
}

/// 多选控件 — 切换时发送 checked_value 或 unchecked_value
export function Checkbox({ widget, onRemove }: CheckboxProps) {
  const { label, checked_value, unchecked_value, binding } = widget.params;
  const checked = useAppStore((s) => {
    const w = s.widgets.find((w) => w.params.id === widget.params.id);
    if (w && w.kind === 'Checkbox') return w.params.default;
    return widget.params.default;
  });
  const updateWidget = useAppStore((s) => s.updateWidget);

  const handleToggle = () => {
    const next = !checked;
    updateWidget(widget.params.id, {
      kind: 'Checkbox',
      params: { ...widget.params, default: next },
    });
    sendBindingValue(binding, next ? checked_value : unchecked_value);
  };

  return (
    <div className="widget-card">
      <button className="btn-icon widget-remove" onClick={onRemove}>
        <X size={12} />
      </button>
      <div className="widget-label">{label}</div>
      <label className="checkbox-item">
        <input type="checkbox" checked={checked} onChange={handleToggle} />
        <span>
          {checked ? checked_value : unchecked_value}
        </span>
      </label>
    </div>
  );
}
