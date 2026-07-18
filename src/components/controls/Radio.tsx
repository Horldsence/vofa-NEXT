import { useEffect } from 'react';
import { X } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { sendBindingValue } from './binding';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';

interface RadioProps {
  widget: Extract<WidgetConfig, { kind: 'Radio' }>;
  onRemove: () => void;
}

/// 单选控件 — 选择选项后发送对应值
/// 当前值通过 setInputValue 推送到后端图 (事件驱动, 供下游 widget 读取)
export function Radio({ widget, onRemove }: RadioProps) {
  const lang = useAppStore((s) => s.lang);
  const { label, options, binding, id } = widget.params;
  const current = useAppStore((s) => {
    const w = s.widgets.find((w) => w.params.id === widget.params.id);
    if (w && w.kind === 'Radio') return w.params.default;
    return widget.params.default;
  });
  const updateWidget = useAppStore((s) => s.updateWidget);
  const setInputValue = useAppStore((s) => s.setInputValue);
  const value = options[current]?.[1] ?? 0;

  const handleChange = (val: number) => {
    updateWidget(widget.params.id, {
      kind: 'Radio',
      params: { ...widget.params, default: val },
    });
    sendBindingValue(binding, val);
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
      <div className="flex flex-col gap-1">
        {options.map(([text, val]) => (
          <label key={val} className="flex items-center gap-1.5 cursor-pointer text-xs">
            <input
              type="radio"
              name={widget.params.id}
              checked={current === val}
              onChange={() => handleChange(val)}
              className="accent-accent"
            />
            <span>{text}</span>
          </label>
        ))}
      </div>
      <div className="text-xs text-text-secondary">
        {t(lang, 'channel')}: {current}
      </div>
    </div>
  );
}
