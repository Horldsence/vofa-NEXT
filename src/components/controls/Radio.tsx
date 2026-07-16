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
export function Radio({ widget, onRemove }: RadioProps) {
  const lang = useAppStore((s) => s.lang);
  const { label, options, binding } = widget.params;
  const current = useAppStore((s) => {
    const w = s.widgets.find((w) => w.params.id === widget.params.id);
    if (w && w.kind === 'Radio') return w.params.default;
    return widget.params.default;
  });
  const updateWidget = useAppStore((s) => s.updateWidget);

  const handleChange = (val: number) => {
    updateWidget(widget.params.id, {
      kind: 'Radio',
      params: { ...widget.params, default: val },
    });
    sendBindingValue(binding, val);
  };

  return (
    <div className="widget-card">
      <button className="btn-icon widget-remove" onClick={onRemove}>
        <X size={12} />
      </button>
      <div className="widget-label">{label}</div>
      <div className="radio-group">
        {options.map(([text, val]) => (
          <label key={val} className="radio-item">
            <input
              type="radio"
              name={widget.params.id}
              checked={current === val}
              onChange={() => handleChange(val)}
            />
            <span>{text}</span>
          </label>
        ))}
      </div>
      <div className="text-xs text-secondary">
        {t(lang, 'channel')}: {current}
      </div>
    </div>
  );
}
