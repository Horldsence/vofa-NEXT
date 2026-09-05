import { memo, useCallback, useEffect, useRef } from 'react';
import type { WidgetConfig } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { snapControlValue } from '../../../lib/utils/numericControl';
import { sendBindingValue } from '../shared/binding';
import { NumericValueInput } from '../shared/NumericValueInput';

interface SliderProps {
  widget: Extract<WidgetConfig, { kind: 'Slider' }>;
}

/// 滑块控件 — 拖拽/滚轮调值, 数值框精确输入
export const Slider = memo(function Slider({ widget }: SliderProps) {
  const { label, min, max, step, binding, id } = widget.params;
  const preview = useAppStore((s) => s.inputPreviewValues[id]);
  const previewInputValue = useAppStore((s) => s.previewInputValue);
  const commitInputValue = useAppStore((s) => s.commitInputValue);
  const setInputValue = useAppStore((s) => s.setInputValue);
  const value = preview ?? widget.params.value;
  const valueRef = useRef(value);
  const inputRef = useRef<HTMLInputElement>(null);
  const wheelTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => { valueRef.current = value; }, [value]);
  useEffect(() => { setInputValue(id, widget.params.value); }, [id, widget.params.value, setInputValue]);

  const previewValue = useCallback((next: number) => {
    previewInputValue(id, snapControlValue(next, { min, max, step }));
  }, [id, max, min, previewInputValue, step]);

  const commitValue = useCallback((next = valueRef.current) => {
    const normalized = snapControlValue(next, { min, max, step });
    commitInputValue(id, normalized);
    sendBindingValue(binding, normalized);
  }, [binding, commitInputValue, id, max, min, step]);

  useEffect(() => {
    const element = inputRef.current;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      event.stopPropagation();
      const direction = event.deltaY < 0 ? 1 : -1;
      const next = snapControlValue(valueRef.current + direction * step, { min, max, step });
      previewInputValue(id, next);
      if (wheelTimerRef.current) clearTimeout(wheelTimerRef.current);
      wheelTimerRef.current = setTimeout(() => commitValue(next), 180);
    };
    element.addEventListener('wheel', onWheel, { passive: false });
    return () => {
      element.removeEventListener('wheel', onWheel);
      if (wheelTimerRef.current) clearTimeout(wheelTimerRef.current);
    };
  }, [commitValue, id, max, min, previewInputValue, step]);

  return (
    <div className="nodrag nowheel flex flex-col gap-1.5 w-full">
      <input
        ref={inputRef}
        type="range"
        className="slider-input nodrag nowheel"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => previewValue(Number(event.target.value))}
        onPointerUp={() => commitValue()}
        onPointerCancel={() => commitValue()}
        onKeyDown={(event) => { event.stopPropagation(); }}
        onKeyUp={(event) => { event.stopPropagation(); commitValue(); }}
        aria-label={label}
      />
      <NumericValueInput value={value} min={min} max={max} step={step} onPreview={previewValue} onCommit={commitValue} />
    </div>
  );
});
