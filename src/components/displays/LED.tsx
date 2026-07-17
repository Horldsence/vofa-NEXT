import { X, Settings2 } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { useWidgetInputValue } from '../../lib/useWidgetInputValue';

interface LEDProps {
  widget: Extract<WidgetConfig, { kind: 'LED' }>;
  onRemove: () => void;
  onEdit?: () => void;
}

/// LED 指示灯 — 输入值 >= threshold 显示 ON 颜色, 否则 OFF
/// 数据源: edge 连线 (上游 widget 输出) 优先, 否则回退到 channel 参数
export function LED({ widget, onRemove, onEdit }: LEDProps) {
  const { label, threshold, on_color, off_color, channel } = widget.params;
  const value = useWidgetInputValue(widget.params.id, 'value', channel, 0);

  const isOn = value >= threshold;
  const color = isOn ? on_color : off_color;

  return (
    <div className="widget-card">
      <button className="btn-icon widget-remove" onClick={onRemove} title="Remove">
        <X size={12} />
      </button>
      {onEdit && (
        <button
          className="btn-icon widget-edit"
          onClick={onEdit}
          title="Edit"
          style={{ right: 24 }}
        >
          <Settings2 size={11} />
        </button>
      )}
      <div className="widget-label">{label}</div>
      <div className="led-container">
        <div
          className={`led ${isOn ? 'led-on' : 'led-off'}`}
          style={{
            background: color,
            boxShadow: isOn ? `0 0 14px ${color}, 0 0 4px ${color}` : 'none',
          }}
        />
        <div className="led-state">{isOn ? 'ON' : 'OFF'}</div>
        <div className="led-value">{value.toFixed(3)}</div>
      </div>
    </div>
  );
}
