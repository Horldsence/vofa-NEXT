import { Settings2 } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { useGraphInput } from '../../lib/useGraphInput';

interface LEDProps {
  widget: Extract<WidgetConfig, { kind: 'LED' }>;
  onRemove: () => void;
  onEdit?: () => void;
}

/// LED 指示灯 — 输入值 >= threshold 显示 ON 颜色, 否则 OFF
/// 数据源: edge 连线 (后端图输出) 优先, 否则回退到 channel 参数
export function LED({ widget, onEdit }: LEDProps) {
  const { threshold, on_color, off_color, channel } = widget.params;
  const value = useGraphInput(widget.params.id, 'value', channel, 0);

  const isOn = value >= threshold;
  const color = isOn ? on_color : off_color;

  return (
    <div className="widget-card">
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
