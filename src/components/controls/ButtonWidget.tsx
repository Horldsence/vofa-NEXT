import { useState, useEffect } from 'react';
import type { WidgetConfig } from '../../types';
import { sendBindingValue } from './binding';
import { useAppStore } from '../../store/appStore';
import clsx from 'clsx';
import { WidgetCard } from '../ui/WidgetCard';

interface ButtonWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'Button' }>;
  onRemove: () => void;
}

/// 按钮控件 — 按下发送 press_value, 释放发送 release_value
/// 当前值通过 setInputValue 推送到后端图 (事件驱动, 供下游 widget 读取)
export function ButtonWidget({ widget, onRemove }: ButtonWidgetProps) {
  const { label, press_value, release_value, binding, id } = widget.params;
  const [pressed, setPressed] = useState(false);
  const setInputValue = useAppStore((s) => s.setInputValue);
  const value = pressed ? press_value : release_value;

  const handleDown = () => {
    setPressed(true);
    sendBindingValue(binding, press_value);
  };
  const handleUp = () => {
    setPressed(false);
    sendBindingValue(binding, release_value);
  };

  // 同步当前值到后端图 (事件驱动)
  useEffect(() => {
    setInputValue(id, value);
  }, [id, value, setInputValue]);

  return (
    <WidgetCard label={label} onRemove={onRemove}>
      <button
        className={clsx(
          "px-4 py-2 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm text-center transition-colors",
          pressed && "bg-bg-button-hover"
        )}
        onPointerDown={handleDown}
        onPointerUp={handleUp}
        onPointerLeave={handleUp}
      >
        {pressed ? press_value : release_value}
      </button>
    </WidgetCard>
  );
}
