import { useRef, useEffect } from 'react';
import type { WidgetConfig } from '../../types';
import { sendBindingValue } from './binding';
import { useAppStore } from '../../store/appStore';
import { WidgetCard } from '../ui/WidgetCard';

interface SliderProps {
  widget: Extract<WidgetConfig, { kind: 'Slider' }>;
  onRemove: () => void;
}

/// 滑块控件 — 拖动调节, 释放时发送值
/// 当前值通过 setInputValue 推送到后端图 (事件驱动, 供下游 widget 读取)
export function Slider({ widget, onRemove }: SliderProps) {
  const { label, min, max, step, binding } = widget.params;
  const value = useAppStore((s) => {
    const w = s.widgets.find((w) => w.params.id === widget.params.id);
    if (w && w.kind === 'Slider') return w.params.default;
    return widget.params.default;
  });
  const updateWidget = useAppStore((s) => s.updateWidget);
  const setInputValue = useAppStore((s) => s.setInputValue);
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

  // 鼠标滚轮调整: 向上加 step, 向下减 step
  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const dir = e.deltaY < 0 ? 1 : -1;
    const increment = step >= 1 ? step : step * 5;
    const raw = value + dir * increment;
    const stepped = Math.round(raw / step) * step;
    const clamped = Math.max(min, Math.min(max, stepped));
    updateWidget(widget.params.id, {
      kind: 'Slider',
      params: { ...widget.params, default: clamped },
    });
    sendBindingValue(binding, clamped);
    lastSentRef.current = clamped;
  };

  // 同步当前值到后端图 (事件驱动, 供下游 widget 读取)
  useEffect(() => {
    setInputValue(widget.params.id, value);
  }, [widget.params.id, value, setInputValue]);

  return (
    <WidgetCard label={label} onRemove={onRemove}>
      <div className="flex flex-col gap-1 w-full">
        <input
          type="range"
          className="slider-input"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={handleChange}
          onPointerUp={handleRelease}
          onKeyUp={handleRelease}
          onWheel={handleWheel}
        />
        <div className="text-xl font-semibold text-text-bright font-mono text-center">{value.toFixed(2)}</div>
      </div>
    </WidgetCard>
  );
}
