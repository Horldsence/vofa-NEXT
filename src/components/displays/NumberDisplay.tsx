import { X, Settings2 } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { useWidgetInputValue } from '../../lib/useWidgetInputValue';

interface NumberDisplayProps {
  widget: Extract<WidgetConfig, { kind: 'NumberDisplay' }>;
  onRemove: () => void;
  onEdit?: () => void;
}

/// 大数字显示 — 大字号展示单通道数值, 含单位与小数位
/// 数据源: edge 连线 (上游 widget 输出) 优先, 否则回退到 channel 参数
export function NumberDisplay({ widget, onRemove, onEdit }: NumberDisplayProps) {
  const { label, unit, precision, channel } = widget.params;
  const value = useWidgetInputValue(widget.params.id, 'value', channel, 0);

  // 自适应字号: 值越长字号越小
  const text = value.toFixed(precision);
  const fontSize = text.length > 10 ? 18 : text.length > 7 ? 24 : 32;

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
      <div className="number-display-container">
        <span className="number-display-value" style={{ fontSize }}>
          {text}
        </span>
        {unit && <span className="number-display-unit">{unit}</span>}
      </div>
      {channel === null && (
        <div className="number-display-hint">未绑定通道</div>
      )}
    </div>
  );
}
