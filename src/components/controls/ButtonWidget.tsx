import { useState } from 'react';
import { X } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { sendBindingValue } from './binding';

interface ButtonWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'Button' }>;
  onRemove: () => void;
}

/// 按钮控件 — 按下发送 press_value, 释放发送 release_value
export function ButtonWidget({ widget, onRemove }: ButtonWidgetProps) {
  const { label, press_value, release_value, binding } = widget.params;
  const [pressed, setPressed] = useState(false);

  const handleDown = () => {
    setPressed(true);
    sendBindingValue(binding, press_value);
  };
  const handleUp = () => {
    setPressed(false);
    sendBindingValue(binding, release_value);
  };

  return (
    <div className="widget-card">
      <button className="btn-icon widget-remove" onClick={onRemove}>
        <X size={12} />
      </button>
      <div className="widget-label">{label}</div>
      <button
        className={`widget-button ${pressed ? 'pressed' : ''}`}
        onPointerDown={handleDown}
        onPointerUp={handleUp}
        onPointerLeave={handleUp}
      >
        {pressed ? press_value : release_value}
      </button>
    </div>
  );
}
