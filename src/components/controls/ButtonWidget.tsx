import { useState, useEffect } from 'react';
import { X } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { sendBindingValue } from './binding';
import { useAppStore } from '../../store/appStore';
import clsx from 'clsx';

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
    <div className="group bg-bg-sidebar border border-border rounded p-2.5 min-w-[140px] flex flex-col gap-1.5 relative">
      <button
        className="absolute top-1 right-1 opacity-0 transition-opacity duration-150 group-hover:opacity-100 w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary"
        onClick={onRemove}
      >
        <X size={12} />
      </button>
      <div className="text-xs text-text-secondary uppercase tracking-[0.3px]">{label}</div>
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
    </div>
  );
}
