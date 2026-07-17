import { useCallback, useRef, useEffect } from 'react';

interface StepKnobProps {
  /// 当前值
  value: number;
  /// 离散档位序列 (1-2-5 序列等)
  steps: number[];
  /// 切换档位回调
  onChange: (newValue: number) => void;
  /// 双击/中心点击时的默认值 (用于重置)
  defaultValue?: number;
  /// 值显示格式化函数
  formatValue: (v: number) => string;
  /// 标签 (显示在旋钮下方)
  label?: string;
  /// 像素直径, 默认 56
  size?: number;
  /// 是否禁用
  disabled?: boolean;
}

/// 离散档位旋钮组件
/// - 拖动垂直方向改变档位 (上 = 加大, 下 = 减小)
/// - 滚轮也可改变档位 (使用原生非被动监听器, 避免页面同时滚动)
/// - 中心点击重置为 defaultValue
/// 旋钮指针角度: 最小档位 = -135°, 最大档位 = +135° (共 270°)
export function StepKnob({
  value,
  steps,
  onChange,
  defaultValue,
  formatValue,
  label,
  size = 56,
  disabled = false,
}: StepKnobProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ startY: number; startIdx: number } | null>(null);

  const currentIndex = steps.indexOf(value);
  const safeIdx = currentIndex >= 0 ? currentIndex : 0;
  const totalSteps = steps.length;
  // -135° 到 +135°, 共 270°
  const angle = totalSteps > 1
    ? (safeIdx / (totalSteps - 1)) * 270 - 135
    : 0;

  const stepTo = useCallback(
    (newIdx: number) => {
      const clamped = Math.max(0, Math.min(totalSteps - 1, newIdx));
      if (clamped !== currentIndex) {
        onChange(steps[clamped]);
      }
    },
    [onChange, steps, totalSteps, currentIndex]
  );

  // 用 ref 保存最新的 stepTo / disabled / safeIdx, 让原生 wheel 监听器能访问最新值
  const stepToRef = useRef(stepTo);
  useEffect(() => { stepToRef.current = stepTo; }, [stepTo]);
  const disabledRef = useRef(disabled);
  useEffect(() => { disabledRef.current = disabled; }, [disabled]);
  const safeIdxRef = useRef(safeIdx);
  useEffect(() => { safeIdxRef.current = safeIdx; }, [safeIdx]);

  // 原生非被动 wheel 监听器 — React onWheel 默认 passive, preventDefault 无效会导致页面滚动
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (disabledRef.current) return;
      e.preventDefault();
      e.stopPropagation();
      const dir = e.deltaY > 0 ? -1 : 1;
      stepToRef.current(safeIdxRef.current + dir);
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, []);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (disabled) return;
      e.preventDefault();
      e.stopPropagation();
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      dragRef.current = { startY: e.clientY, startIdx: safeIdx };
    },
    [disabled, safeIdx]
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!dragRef.current || disabled) return;
      const dy = dragRef.current.startY - e.clientY;
      // 每 12px 一档
      const delta = Math.round(dy / 12);
      stepTo(dragRef.current.startIdx + delta);
    },
    [disabled, stepTo]
  );

  const handlePointerUp = useCallback(
    (e: React.PointerEvent) => {
      if (dragRef.current) {
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
        dragRef.current = null;
      }
    },
    []
  );

  const handleDoubleClick = useCallback(() => {
    if (disabled) return;
    if (defaultValue !== undefined && defaultValue !== value) {
      onChange(defaultValue);
    }
  }, [disabled, defaultValue, value, onChange]);

  return (
    <div
      ref={containerRef}
      className={`scope-knob ${disabled ? 'disabled' : ''}`}
      style={{ width: size }}
    >
      <div
        className="knob-dial"
        style={{
          width: size,
          height: size,
          transform: `rotate(${angle}deg)`,
        }}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onDoubleClick={handleDoubleClick}
      >
        <div className="knob-pointer" />
        <div className="knob-rim" />
      </div>
      <div className="knob-value" title={formatValue(value)}>
        {formatValue(value)}
      </div>
      {label && <div className="knob-label">{label}</div>}
    </div>
  );
}
