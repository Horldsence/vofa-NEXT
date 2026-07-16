import { useRef } from 'react';
import { X } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { sendBindingValue } from './binding';
import { useAppStore } from '../../store/appStore';

interface SliderProps {
  widget: Extract<WidgetConfig, { kind: 'Slider' }>;
  onRemove: () => void;
}

/// 滑块控件 — 拖动调节, 释放时发送值
export function Slider({ widget, onRemove }: SliderProps) {
  const { label, min, max, step, binding } = widget.params;
  const value = useAppStore((s) => {
    const w = s.widgets.find((w) => w.params.id === widget.params.id);
    if (w && w.kind === 'Slider') return w.params.default;
    return widget.params.default;
  });
  const updateWidget = useAppStore((s) => s.updateWidget);
  const lastSentRef = useRef(value);

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const v = parseFloat(e.target.value);
    updateWidget(widget.params.id, {
      kind: 'Slider',
      params: { ...widget.params, default: v },
    });
  };

  const handleRelease = () => {
    if (value !== lastSentRef.current) {
      sendBindingValue(binding, value);
      lastSentRef.current = value;
    }
  };

  return (
    <div className="widget-card">
      <button className="btn-icon widget-remove" onClick={onRemove}>
        <X size={12} />
      </button>
      <div className="widget-label">{label}</div>
      <div className="slider-container">
        <input
          type="range"
          className="slider"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={handleChange}
          onPointerUp={handleRelease}
          onKeyUp={handleRelease}
        />
        <div className="widget-value">{value.toFixed(2)}</div>
      </div>
    </div>
  );
}
