import { useCallback, useEffect, useRef } from 'react';
import type { WidgetConfig } from '../../types';
import { useAppStore } from '../../store/appStore';
import { snapControlValue } from '../../lib/utils/numericControl';
import { WidgetCard } from '../ui/WidgetCard';
import { sendBindingValue } from './binding';
import { NumericValueInput } from './NumericValueInput';

interface KnobProps {
  widget: Extract<WidgetConfig, { kind: 'Knob' }>;
  onRemove: () => void;
}

export function Knob({ widget, onRemove }: KnobProps) {
  const { label, min, max, step, binding, id } = widget.params;
  const preview = useAppStore((s) => s.inputPreviewValues[id]);
  const previewInputValue = useAppStore((s) => s.previewInputValue);
  const commitInputValue = useAppStore((s) => s.commitInputValue);
  const setInputValue = useAppStore((s) => s.setInputValue);
  const value = preview ?? widget.params.value;
  const valueRef = useRef(value);
  const knobRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ pointerId: number; startY: number; startValue: number } | null>(null);
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

  const finishPointer = (event: React.PointerEvent<HTMLDivElement>) => {
    if (dragRef.current?.pointerId !== event.pointerId) return;
    dragRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    commitValue();
  };

  useEffect(() => {
    const element = knobRef.current;
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

  const angle = ((value - min) / (max - min)) * 270 - 135;

  return (
    <WidgetCard label={label} onRemove={onRemove}>
      <div className="nodrag nowheel flex flex-col items-center gap-1.5">
        <div
          ref={knobRef}
          className="knob-dial nodrag nowheel"
          role="slider"
          tabIndex={0}
          aria-label={label}
          aria-valuemin={min}
          aria-valuemax={max}
          aria-valuenow={value}
          onPointerDown={(event) => {
            event.stopPropagation();
            dragRef.current = { pointerId: event.pointerId, startY: event.clientY, startValue: valueRef.current };
            event.currentTarget.setPointerCapture(event.pointerId);
          }}
          onPointerMove={(event) => {
            const drag = dragRef.current;
            if (drag?.pointerId !== event.pointerId) return;
            const raw = drag.startValue + ((drag.startY - event.clientY) / 120) * (max - min);
            previewValue(raw);
          }}
          onPointerUp={finishPointer}
          onPointerCancel={finishPointer}
          onKeyDown={(event) => {
            event.stopPropagation();
            let next: number | null = null;
            if (event.key === 'ArrowUp' || event.key === 'ArrowRight') next = valueRef.current + step;
            if (event.key === 'ArrowDown' || event.key === 'ArrowLeft') next = valueRef.current - step;
            if (event.key === 'Home') next = min;
            if (event.key === 'End') next = max;
            if (next !== null) {
              event.preventDefault();
              previewValue(next);
            }
          }}
          onKeyUp={(event) => {
            if (['ArrowUp', 'ArrowRight', 'ArrowDown', 'ArrowLeft', 'Home', 'End'].includes(event.key)) {
              event.stopPropagation();
              commitValue();
            }
          }}
        >
          <div className="knob-indicator-line" style={{ transform: `translateX(-50%) rotate(${angle}deg)` }} />
        </div>
        <NumericValueInput value={value} min={min} max={max} step={step} onPreview={previewValue} onCommit={commitValue} />
      </div>
    </WidgetCard>
  );
}
