import { useRef, useState, useEffect } from 'react';
import { X } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { sendBindingValue } from './binding';

interface KnobProps {
  widget: Extract<WidgetConfig, { kind: 'Knob' }>;
  onRemove: () => void;
}

/// 旋钮控件 — 拖动调节角度, 释放时发送值
export function Knob({ widget, onRemove }: KnobProps) {
  const { label, min, max, step, default: def, binding } = widget.params;
  const [value, setValue] = useState(def);
  const knobRef = useRef<HTMLDivElement>(null);
  const draggingRef = useRef(false);

  // 角度范围: -135° 到 +135° (270° 总行程)
  const angle = ((value - min) / (max - min)) * 270 - 135;

  const handlePointerDown = (e: React.PointerEvent) => {
    draggingRef.current = true;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  };

  const handlePointerMove = (e: React.PointerEvent) => {
    if (!draggingRef.current || !knobRef.current) return;
    const rect = knobRef.current.getBoundingClientRect();
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    const dx = e.clientX - cx;
    const dy = e.clientY - cy;
    // 计算角度 (0 在顶部, 顺时针为正)
    let deg = Math.atan2(dx, -dy) * 180 / Math.PI;
    // 限制在 -135 到 +135
    if (deg > 135) deg = 135;
    if (deg < -135) deg = -135;
    const ratio = (deg + 135) / 270;
    const raw = min + ratio * (max - min);
    const stepped = Math.round(raw / step) * step;
    const clamped = Math.max(min, Math.min(max, stepped));
    setValue(clamped);
  };

  const handlePointerUp = () => {
    if (draggingRef.current) {
      draggingRef.current = false;
      sendBindingValue(binding, value);
    }
  };

  // 切换控件时重置默认值
  useEffect(() => {
    setValue(def);
  }, [def]);

  return (
    <div className="widget-card">
      <button className="btn-icon widget-remove" onClick={onRemove}>
        <X size={12} />
      </button>
      <div className="widget-label">{label}</div>
      <div className="knob-container">
        <div
          ref={knobRef}
          className="knob"
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
        >
          <div
            className="knob-indicator"
            style={{ transform: `translateX(-50%) rotate(${angle}deg)` }}
          />
        </div>
        <div className="widget-value">{value.toFixed(2)}</div>
      </div>
    </div>
  );
}
