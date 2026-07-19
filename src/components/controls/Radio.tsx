import { useEffect } from 'react';
import type { WidgetConfig } from '../../types';
import { sendBindingValue } from './binding';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { WidgetCard } from '../ui/WidgetCard';

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
    <WidgetCard label={label} onRemove={onRemove}>
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
    </WidgetCard>
  );
}
